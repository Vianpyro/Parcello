//! The room's clocks (ADR-0010/0023/0028): per-turn AFK deadline, the
//! personal time bank it drains, the game clock, and the animation-ack
//! watermark that gates the other timers on what clients have rendered.

use std::time::Duration;

use super::{
    ANIM_ACK_CAP, BID_WINDOW, DISCONNECTED_GRACE, JAIL_DECISION_SECS, Phase, Room, VOTE_WINDOW,
};

impl Room {
    /// Whether `seat` has rendered the latest Update. Bot and disconnected
    /// seats have no visual to wait for and never gate anyone - the same
    /// "I don't animate" path the CLI takes by acking instantly.
    pub(super) fn seat_settled(&self, seat: usize) -> bool {
        let Some(s) = self.seats.get(seat) else {
            return true;
        };
        s.is_bot || s.tx.is_none() || self.acked.get(seat).copied().unwrap_or(0) >= self.seq
    }

    /// Stamps the table/acting settle instants once their condition holds -
    /// every relevant ack arrived, or `ANIM_ACK_CAP` elapsed, whichever
    /// comes first - then arms any window deadline that was waiting on the
    /// table. Runs at the top of every loop iteration, so both an incoming
    /// ack (a `RoomCmd`) and the cap wake-up are observed promptly.
    pub(super) fn refresh_gates(&mut self) {
        let now = tokio::time::Instant::now();
        let cap = self.anim_broadcast_at + ANIM_ACK_CAP;
        let capped = now >= cap;
        let stamp = if capped { cap } else { now };
        if self.table_settled_at.is_none()
            && (capped || (0..self.seats.len()).all(|s| self.seat_settled(s)))
        {
            self.table_settled_at = Some(stamp);
        }
        if self.acting_settled_at.is_none()
            && (capped || self.acting_seat().is_none_or(|s| self.seat_settled(s)))
        {
            self.acting_settled_at = Some(stamp);
        }
        // A collection window's clock starts only once the table has
        // visually arrived (ADR-0018/0024 windows, gated per ADR-0028).
        if let Some(t) = self.table_settled_at {
            if self.bid_gate {
                self.bid_gate = false;
                self.bid_deadline = Some(t + BID_WINDOW);
            }
            if self.vote_gate {
                self.vote_gate = false;
                self.vote_deadline = Some(t + VOTE_WINDOW);
            }
        }
    }

    /// Turn-clock anchor (ADR-0028): the clock starts at the later of the
    /// last accepted command and the acting seat's own render ack (or the
    /// cap) - rendering time never eats thinking time.
    pub(super) fn acting_anchor(
        &self,
        last_progress: tokio::time::Instant,
    ) -> tokio::time::Instant {
        let settled = self
            .acting_settled_at
            .unwrap_or(self.anim_broadcast_at + ANIM_ACK_CAP);
        last_progress.max(settled)
    }

    /// Bot pacing anchor (ADR-0028): a bot moves only once the whole table
    /// has rendered the previous move (or the cap) - otherwise bots race
    /// ahead of what the humans can see.
    pub(super) fn table_anchor(&self, last_progress: tokio::time::Instant) -> tokio::time::Instant {
        let settled = self
            .table_settled_at
            .unwrap_or(self.anim_broadcast_at + ANIM_ACK_CAP);
        last_progress.max(settled)
    }

    /// Records a client's "rendered through N" ack (ADR-0028). `through_seq`
    /// is untrusted wire input: clamped to what was actually sent, and only
    /// ever raises the seat's watermark (acking can only release timers
    /// earlier, never delay anything).
    pub(super) fn handle_animation_done(&mut self, player_id: &str, through_seq: u64) {
        let Some(seat) = self.seat_of(player_id) else {
            return;
        };
        if self.acked.len() < self.seats.len() {
            self.acked.resize(self.seats.len(), 0);
        }
        let acked = through_seq.min(self.seq);
        if let Some(a) = self.acked.get_mut(seat) {
            *a = (*a).max(acked);
        }
    }

    /// Bumps the Update counter (every broadcast Update carries it) and
    /// closes the animation gates: a fresh broadcast means fresh visuals
    /// the table has not rendered yet (ADR-0028).
    pub(super) fn next_update_seq(&mut self) -> u64 {
        self.seq += 1;
        self.anim_broadcast_at = tokio::time::Instant::now();
        self.table_settled_at = None;
        self.acting_settled_at = None;
        self.seq
    }

    /// Effective plain-turn limit for `seat` right now, or `None` when the
    /// room has no turn limit at all. Normally `settings.turn_seconds`,
    /// floored to `JAIL_DECISION_SECS` for a jailed seat still choosing its
    /// exit (`jail_route.is_none()` - once any exit is chosen the player
    /// either un-jails immediately or, on a failed bribe, is back at this
    /// same decision next turn, so the floor reapplies correctly either
    /// way). Shared by `afk_deadline` and `drain_bank` so the auto-play
    /// trigger and the bank drain always agree on the same budget.
    pub(super) fn turn_limit_secs(&self, seat: usize) -> Option<u64> {
        let base = self.settings.turn_seconds?;
        let Phase::Active(state) = &self.phase else {
            return Some(base);
        };
        let jailed_deciding = state
            .players
            .get(seat)
            .is_some_and(|p| p.jailed && p.jail_route.is_none());
        Some(if jailed_deciding {
            base.max(JAIL_DECISION_SECS)
        } else {
            base
        })
    }

    /// How long the acting seat may stall before its canonical action is
    /// auto-played, or `None` for no limit. A disconnected seat (truly AFK)
    /// is skipped after `DISCONNECTED_GRACE` whether or not a turn limit is
    /// set - the personal time bank does not apply to them (ADR-0023,
    /// pulling the plug earns no extra time). A connected but slow player
    /// gets the room's turn limit extended by whatever bank they have left.
    pub(super) fn afk_deadline(&self) -> Option<Duration> {
        let seat = self.acting_seat()?;
        let turn_limit = self.turn_limit_secs(seat).map(Duration::from_secs);
        let connected = self.seats.get(seat).is_some_and(|s| s.tx.is_some());
        if connected {
            let bank = Duration::from_secs(self.banks.get(seat).copied().unwrap_or(0));
            turn_limit.map(|t| t + bank)
        } else {
            Some(turn_limit.map_or(DISCONNECTED_GRACE, |t| t.min(DISCONNECTED_GRACE)))
        }
    }

    /// Drains `seat`'s personal time bank by however long it overran the
    /// plain turn window (ADR-0023) and returns the amount actually taken,
    /// for a caller that may need to `refund_bank` it back. A no-op (returns
    /// 0) with no turn limit, no bank, no overage, or a disconnected seat
    /// (whose timeout is governed by `DISCONNECTED_GRACE` alone).
    pub(super) fn drain_bank(&mut self, seat: Option<usize>, elapsed: Duration) -> u64 {
        let Some(seat) = seat else { return 0 };
        if self.seats.get(seat).is_none_or(|s| s.tx.is_none()) {
            return 0;
        }
        let Some(turn_limit) = self.turn_limit_secs(seat).map(Duration::from_secs) else {
            return 0;
        };
        let overage = elapsed.saturating_sub(turn_limit).as_secs();
        let Some(remaining) = self.banks.get_mut(seat) else {
            return 0;
        };
        let drained = overage.min(*remaining);
        *remaining -= drained;
        drained
    }

    /// Undoes a `drain_bank` call whose command turned out rejected -
    /// rejections never mutate (the codebase-wide invariant), and that
    /// includes this session-layer side effect too.
    pub(super) fn refund_bank(&mut self, seat: Option<usize>, amount: u64) {
        if amount == 0 {
            return;
        }
        if let Some(seat) = seat
            && let Some(remaining) = self.banks.get_mut(seat)
        {
            *remaining += amount;
        }
    }

    /// Seconds left before the game clock ends the game, if time-boxed.
    pub(super) fn time_remaining_secs(&self) -> Option<u64> {
        self.game_deadline.map(|d| {
            d.saturating_duration_since(tokio::time::Instant::now())
                .as_secs()
        })
    }

    /// The configured time bank, normalized so `Some(0)` (host explicitly
    /// set it to zero via `Configure`) and `None` both read as "disabled"
    /// everywhere this rides the wire (ADR-0023).
    pub(super) fn configured_time_bank(&self) -> Option<u64> {
        self.settings.time_bank_seconds.filter(|&s| s > 0)
    }

    /// Live per-seat remaining bank for `Update.banks`; `None` when the
    /// room has no time bank configured.
    pub(super) fn banks_field(&self) -> Option<Vec<u64>> {
        self.configured_time_bank().map(|_| self.banks.clone())
    }
}
