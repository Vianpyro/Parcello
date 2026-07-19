//! Persistence port for the per-server ladder (ADR-0034, Repository
//! pattern like `GameHistory`).
//!
//! A separate port from `GameHistory` on purpose: history is append-only
//! and fire-and-forget (ADR-0005), ratings are read-modify-write (read at
//! queue entry, update at game end). The end-of-game update is one small
//! synchronous transaction per finished game - rare enough that a writer
//! thread would be ceremony; it is never on the per-command hot path.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use parcello_protocol::RatingChange;
use tracing::warn;

use super::ladder::{self, Rating};

/// One player's ladder record.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerRating {
    pub rating: Rating,
    pub games: u64,
    pub wins: u64,
}

pub trait RatingStore: Send + Sync {
    /// Current record for a player; the Weng-Lin defaults when unknown.
    fn get(&self, player_id: &str) -> PlayerRating;
    /// Applies one finished rated game. `ordered` is best-to-worst
    /// (index 0 won); returns the per-player changes in the same order.
    fn record_match(&self, room: &str, ordered: &[String]) -> Vec<RatingChange>;
}

/// Shared by both adapters: read current ratings, run the pure Weng-Lin
/// update, and describe the change for the wire.
fn compute_changes(before: &[PlayerRating], ordered: &[String]) -> Vec<RatingChange> {
    let ratings: Vec<Rating> = before.iter().map(|p| p.rating).collect();
    let after = ladder::rate(&ratings);
    ordered
        .iter()
        .zip(ratings.iter().zip(&after))
        .map(|(id, (old, new))| RatingChange {
            player_id: id.clone(),
            mu: new.mu,
            sigma: new.sigma,
            display: ladder::display(*new),
            display_delta: ladder::display(*new) - ladder::display(*old),
        })
        .collect()
}

/// In-memory adapter: tests, and `--ranked` without persistence (the boot
/// warning tells the operator the ladder dies with the process).
#[derive(Default)]
pub struct MemoryRatings {
    records: Mutex<HashMap<String, PlayerRating>>,
}

impl MemoryRatings {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl RatingStore for MemoryRatings {
    fn get(&self, player_id: &str) -> PlayerRating {
        self.records
            .lock()
            .expect("ratings mutex poisoned")
            .get(player_id)
            .copied()
            .unwrap_or_default()
    }

    fn record_match(&self, _room: &str, ordered: &[String]) -> Vec<RatingChange> {
        let mut records = self.records.lock().expect("ratings mutex poisoned");
        let before: Vec<PlayerRating> = ordered
            .iter()
            .map(|id| records.get(id).copied().unwrap_or_default())
            .collect();
        let changes = compute_changes(&before, ordered);
        for (i, (id, change)) in ordered.iter().zip(&changes).enumerate() {
            let entry = records.entry(id.clone()).or_default();
            entry.rating = Rating {
                mu: change.mu,
                sigma: change.sigma,
            };
            entry.games += 1;
            entry.wins += u64::from(i == 0);
        }
        drop(records);
        changes
    }
}

/// `SQLite` adapter.
///
/// Opens its own connection - safe to point at the same file as
/// `SqliteHistory` (both run WAL). Access is low-frequency (one read per
/// queue entry, one transaction per finished game), so a plain mutexed
/// connection is enough.
pub struct SqliteRatings {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteRatings {
    /// # Errors
    /// When the database file cannot be opened/created or the schema
    /// migration fails.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS rating (
                 player_id  TEXT PRIMARY KEY,
                 mu         REAL NOT NULL,
                 sigma      REAL NOT NULL,
                 games      INTEGER NOT NULL,
                 wins       INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS rated_game (
                 room       TEXT NOT NULL,
                 player_id  TEXT NOT NULL,
                 placement  INTEGER NOT NULL,
                 mu_before  REAL NOT NULL,
                 sigma_before REAL NOT NULL,
                 mu_after   REAL NOT NULL,
                 sigma_after REAL NOT NULL,
                 at         INTEGER NOT NULL
             );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn read_one(conn: &rusqlite::Connection, player_id: &str) -> PlayerRating {
        let row = conn.query_row(
            "SELECT mu, sigma, games, wins FROM rating WHERE player_id = ?1",
            [player_id],
            |r| {
                Ok(PlayerRating {
                    rating: Rating {
                        mu: r.get(0)?,
                        sigma: r.get(1)?,
                    },
                    games: r.get::<_, i64>(2)? as u64,
                    wins: r.get::<_, i64>(3)? as u64,
                })
            },
        );
        match row {
            Ok(record) => record,
            // An unknown player really is a fresh default...
            Err(rusqlite::Error::QueryReturnedNoRows) => PlayerRating::default(),
            // ...but any other failure (locked/corrupt database) must not
            // silently reset an established rating - make it loud, even
            // though the defaults still let play continue.
            Err(e) => {
                warn!(error = %e, player_id, "rating read failed; using defaults");
                PlayerRating::default()
            }
        }
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl RatingStore for SqliteRatings {
    fn get(&self, player_id: &str) -> PlayerRating {
        let conn = self.conn.lock().expect("ratings mutex poisoned");
        Self::read_one(&conn, player_id)
    }

    fn record_match(&self, room: &str, ordered: &[String]) -> Vec<RatingChange> {
        let mut conn = self.conn.lock().expect("ratings mutex poisoned");
        let before: Vec<PlayerRating> =
            ordered.iter().map(|id| Self::read_one(&conn, id)).collect();
        let changes = compute_changes(&before, ordered);
        let at = now_unix();
        let write = |tx: &rusqlite::Transaction| -> Result<(), rusqlite::Error> {
            for (place, ((id, old), change)) in
                ordered.iter().zip(&before).zip(&changes).enumerate()
            {
                tx.execute(
                    "INSERT INTO rating (player_id, mu, sigma, games, wins, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(player_id) DO UPDATE SET
                         mu = ?2, sigma = ?3, games = ?4, wins = ?5, updated_at = ?6",
                    rusqlite::params![
                        id,
                        change.mu,
                        change.sigma,
                        (old.games + 1) as i64,
                        (old.wins + u64::from(place == 0)) as i64,
                        at
                    ],
                )?;
                tx.execute(
                    "INSERT INTO rated_game
                     (room, player_id, placement, mu_before, sigma_before,
                      mu_after, sigma_after, at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        room,
                        id,
                        (place + 1) as i64,
                        old.rating.mu,
                        old.rating.sigma,
                        change.mu,
                        change.sigma,
                        at
                    ],
                )?;
            }
            Ok(())
        };
        // Best-effort like history writes (ADR-0005): a failed write is
        // logged, the broadcast still goes out with the computed changes.
        match conn.transaction() {
            Ok(tx) => {
                if let Err(e) = write(&tx).and_then(|()| tx.commit()) {
                    warn!(error = %e, room, "rating write failed");
                }
            }
            Err(e) => warn!(error = %e, room, "rating transaction failed"),
        }
        drop(conn);
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|&n| format!("id:{n}")).collect()
    }

    #[test]
    fn memory_store_tracks_games_wins_and_moves_ratings() {
        let store = MemoryRatings::new();
        let players = ids(&["a", "b", "c"]);

        let changes = store.record_match("ROOM1", &players);
        assert_eq!(changes.len(), 3);
        assert!(changes[0].display_delta > 0, "winner climbs");
        assert!(changes[2].display_delta <= 0, "last place does not climb");

        let winner = store.get("id:a");
        assert_eq!((winner.games, winner.wins), (1, 1));
        let loser = store.get("id:c");
        assert_eq!((loser.games, loser.wins), (1, 0));
        assert_eq!(store.get("id:unknown").games, 0, "unknown = defaults");
    }

    #[test]
    fn sqlite_store_persists_across_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("ratings.db");
        let players = ids(&["x", "y"]);

        let store = SqliteRatings::open(&db).expect("open");
        let changes = store.record_match("KAZOO", &players);
        drop(store);

        let store = SqliteRatings::open(&db).expect("reopen");
        let x = store.get("id:x");
        assert_eq!((x.games, x.wins), (1, 1));
        assert!((x.rating.mu - changes[0].mu).abs() < 1e-9);
        assert!(x.rating.mu > Rating::default().mu);

        // The audit table keeps the per-game trajectory.
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM rated_game", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 2);
    }

    #[test]
    fn stores_agree_on_the_math() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sqlite = SqliteRatings::open(&dir.path().join("r.db")).expect("open");
        let memory = MemoryRatings::new();
        let players = ids(&["a", "b", "c", "d"]);

        let from_sqlite = sqlite.record_match("ROOM", &players);
        let from_memory = memory.record_match("ROOM", &players);
        for (s, m) in from_sqlite.iter().zip(&from_memory) {
            assert!((s.mu - m.mu).abs() < 1e-12);
            assert!((s.sigma - m.sigma).abs() < 1e-12);
            assert_eq!(s.display, m.display);
        }
    }
}
