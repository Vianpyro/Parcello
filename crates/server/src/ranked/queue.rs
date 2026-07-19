//! The ranked queue and the matchmaker task (ADR-0034).
//!
//! Connections enter through `ws.rs` (token identities only), wait in a
//! single server-wide pool, and leave either by cancelling/disconnecting or
//! because the matchmaker carved a table out of the pool and created a
//! ranked room for it. Matching itself (`propose_match`) is pure and
//! clock-free so the widening-window policy is unit-testable.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use parcello_protocol::ServerMessage;
use tokio::time::Instant;
use tracing::{info, warn};

use super::store::RatingStore;
use crate::AppState;
use crate::auth::Identity;
use crate::room::{self, ClientTx, MIN_PLAYERS, RankedSetup};

/// Matchmaking policy knobs. Constants (not flags) until playtests say
/// otherwise (ADR-0034); tests shrink them to run fast.
#[derive(Debug, Clone, Copy)]
pub struct RankedConfig {
    /// Preferred table size; a full window match starts immediately.
    pub target_seats: usize,
    /// Once the oldest entry has waited this long, any `MIN_PLAYERS`
    /// compatible entries make a table.
    pub fallback: Duration,
    /// How long a ranked room waits in its lobby for matched players before
    /// starting without the absentees (or aborting below `MIN_PLAYERS`).
    pub start_grace: Duration,
    /// Matchmaker cadence.
    pub tick: Duration,
}

impl Default for RankedConfig {
    fn default() -> Self {
        Self {
            target_seats: 4,
            fallback: Duration::from_mins(1),
            start_grace: Duration::from_secs(15),
            tick: Duration::from_secs(2),
        }
    }
}

/// One waiting player. Arrival order is preserved: index 0 is the oldest
/// entry and anchors the matching pass.
struct Entry {
    identity: Identity,
    mu: f64,
    tx: ClientTx,
    since: Instant,
}

/// A queue candidate as the pure matcher sees it.
struct Candidate {
    mu: f64,
    waited: Duration,
}

/// Rating tolerance around one entry, in mu units: starts at 2.0 and widens
/// by 1.0 per 10 seconds waited (ADR-0034).
const fn window(c: &Candidate) -> f64 {
    c.waited.as_secs_f64().mul_add(0.1, 2.0)
}

/// Pure matching pass: the oldest entry anchors; entries are compatible
/// when their mu gap fits inside *both* windows. A full `target` table
/// matches immediately (closest mu first); after `fallback` the anchor
/// takes any `MIN_PLAYERS` compatible entries. Returns indices into
/// `cands`, anchor first.
fn propose_match(cands: &[Candidate], target: usize, fallback: Duration) -> Option<Vec<usize>> {
    let anchor = cands.first()?;
    let mut compatible: Vec<usize> = (1..cands.len())
        .filter(|&i| {
            let gap = (cands[i].mu - anchor.mu).abs();
            gap <= window(anchor).min(window(&cands[i]))
        })
        .collect();
    compatible.sort_by(|&a, &b| {
        let da = (cands[a].mu - anchor.mu).abs();
        let db = (cands[b].mu - anchor.mu).abs();
        da.total_cmp(&db)
    });
    let mut picked = vec![0];
    picked.extend(compatible.into_iter().take(target - 1));
    if picked.len() >= target || (anchor.waited >= fallback && picked.len() >= MIN_PLAYERS) {
        Some(picked)
    } else {
        None
    }
}

/// Server-wide ranked state: the waiting pool plus the rating store the
/// matchmaker and the rooms both consult. Lives in `AppState` when
/// `--ranked` is on.
pub struct RankedService {
    pub store: Arc<dyn RatingStore>,
    pub config: RankedConfig,
    entries: Mutex<Vec<Entry>>,
}

impl RankedService {
    #[must_use]
    pub fn new(store: Arc<dyn RatingStore>, config: RankedConfig) -> Arc<Self> {
        Arc::new(Self {
            store,
            config,
            entries: Mutex::new(Vec::new()),
        })
    }

    /// Adds (or refreshes) a waiting player and tells every queued
    /// connection the new pool size.
    ///
    /// # Panics
    /// If the queue mutex was poisoned by a panicking holder.
    pub fn enqueue(&self, identity: Identity, mu: f64, tx: ClientTx) {
        let mut entries = self.entries.lock().expect("queue mutex poisoned");
        entries.retain(|e| e.identity.player_id != identity.player_id);
        entries.push(Entry {
            identity,
            mu,
            tx,
            since: Instant::now(),
        });
        Self::broadcast_size(&entries);
        drop(entries);
    }

    /// Drops a waiting player (cancel, disconnect, or entering a room).
    ///
    /// Scoped to the calling connection (`tx`): if the same identity
    /// re-queued from a newer connection, that fresh entry survives the
    /// old connection's cleanup. A no-op for ids that already left or
    /// were matched.
    ///
    /// # Panics
    /// If the queue mutex was poisoned by a panicking holder.
    pub fn remove(&self, player_id: &str, tx: &ClientTx) {
        let mut entries = self.entries.lock().expect("queue mutex poisoned");
        let before = entries.len();
        entries.retain(|e| e.identity.player_id != player_id || !e.tx.same_channel(tx));
        if entries.len() != before {
            Self::broadcast_size(&entries);
        }
    }

    /// Current pool size (post-cancel confirmation for the client).
    ///
    /// # Panics
    /// If the queue mutex was poisoned by a panicking holder.
    pub fn len(&self) -> usize {
        self.entries.lock().expect("queue mutex poisoned").len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn broadcast_size(entries: &[Entry]) {
        let size = entries.len();
        for e in entries {
            let _ = e.tx.send(ServerMessage::Queued { size });
        }
    }

    /// Drops entries whose connection is gone, then carves one table out of
    /// the pool if the policy allows it.
    fn take_match(&self, now: Instant) -> Option<Vec<Entry>> {
        let mut entries = self.entries.lock().expect("queue mutex poisoned");
        let before = entries.len();
        entries.retain(|e| !e.tx.is_closed());
        if entries.len() != before {
            Self::broadcast_size(&entries);
        }
        let cands: Vec<Candidate> = entries
            .iter()
            .map(|e| Candidate {
                mu: e.mu,
                waited: now.saturating_duration_since(e.since),
            })
            .collect();
        let mut picked = propose_match(&cands, self.config.target_seats, self.config.fallback)?;
        // Remove back-to-front so earlier indices stay valid.
        picked.sort_unstable_by(|a, b| b.cmp(a));
        let mut matched: Vec<Entry> = picked.into_iter().map(|i| entries.remove(i)).collect();
        matched.reverse(); // anchor first again
        if !entries.is_empty() {
            Self::broadcast_size(&entries);
        }
        drop(entries);
        Some(matched)
    }

    /// Puts a failed match back at the front of the pool, oldest first.
    fn requeue(&self, matched: Vec<Entry>) {
        let mut entries = self.entries.lock().expect("queue mutex poisoned");
        for (i, e) in matched.into_iter().enumerate() {
            let pos = i.min(entries.len());
            entries.insert(pos, e);
        }
    }
}

/// Boots the matchmaker task for this server. A no-op when `--ranked` is
/// off (`AppState.ranked` is `None`).
pub fn spawn_matchmaker(app: AppState) {
    let Some(service) = app.ranked.clone() else {
        return;
    };
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(service.config.tick);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            while let Some(matched) = service.take_match(Instant::now()) {
                launch_table(&app, &service, matched).await;
            }
        }
    });
}

/// Creates the ranked room for a formed table and tells each player where
/// to go. The clients answer with a normal `Join` (the transport never
/// teleports a connection into a room, architecture section 5.1).
async fn launch_table(app: &AppState, service: &RankedService, matched: Vec<Entry>) {
    let setup = RankedSetup {
        expected: matched.iter().map(|e| e.identity.clone()).collect(),
        store: Arc::clone(&service.store),
        start_grace: service.config.start_grace,
    };
    let created = room::create_ranked_room(
        &app.rooms,
        Arc::clone(&app.content),
        Arc::clone(&app.history),
        app.turn_timeout,
        app.time_bank,
        app.game_timeout,
        setup,
    )
    .await;
    match created {
        Ok(code) => {
            info!(room = %code, seats = matched.len(), "ranked table formed");
            for e in &matched {
                let _ = e.tx.send(ServerMessage::MatchFound { code: code.clone() });
            }
        }
        Err(e) => {
            // Cannot really happen (the server's content validated at boot),
            // but nobody should vanish from the queue over it.
            warn!(error = %e, "ranked room creation failed; players requeued");
            service.requeue(matched);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(mu: f64, waited_secs: u64) -> Candidate {
        Candidate {
            mu,
            waited: Duration::from_secs(waited_secs),
        }
    }

    const TARGET: usize = 4;
    const FALLBACK: Duration = Duration::from_mins(1);

    #[test]
    fn a_full_window_matches_immediately_closest_first() {
        // Anchor at mu 25; three compatible entries and one outlier.
        let cands = vec![
            cand(25.0, 0),
            cand(40.0, 0), // outlier: gap 15 > window
            cand(26.5, 0), // gap 1.5
            cand(24.0, 0), // gap 1.0
            cand(23.8, 0), // gap 1.2
        ];
        let picked = propose_match(&cands, TARGET, FALLBACK).expect("match");
        assert_eq!(picked, vec![0, 3, 4, 2], "anchor then closest mu first");
    }

    #[test]
    fn short_queues_wait_until_the_fallback() {
        let young = vec![cand(25.0, 5), cand(25.0, 5)];
        assert!(
            propose_match(&young, TARGET, FALLBACK).is_none(),
            "two fresh entries keep waiting for a fuller table"
        );

        let old = vec![cand(25.0, 61), cand(25.0, 5)];
        let picked = propose_match(&old, TARGET, FALLBACK).expect("fallback match");
        assert_eq!(picked, vec![0, 1], "past the fallback, two make a table");
    }

    #[test]
    fn windows_widen_with_wait_and_bind_both_sides() {
        // Gap of 5 mu: too far for fresh entries...
        let fresh = vec![cand(20.0, 0), cand(25.0, 0)];
        assert!(propose_match(&fresh, 2, FALLBACK).is_none());

        // ...compatible once BOTH have waited enough (min of the windows).
        let one_sided = vec![cand(20.0, 40), cand(25.0, 0)];
        assert!(
            propose_match(&one_sided, 2, FALLBACK).is_none(),
            "a fresh opponent's narrow window still binds"
        );
        let both = vec![cand(20.0, 40), cand(25.0, 31)];
        assert!(propose_match(&both, 2, FALLBACK).is_some());
    }

    #[test]
    fn an_empty_queue_matches_nobody() {
        assert!(propose_match(&[], TARGET, FALLBACK).is_none());
        assert!(propose_match(&[cand(25.0, 999)], TARGET, FALLBACK).is_none());
    }
}
