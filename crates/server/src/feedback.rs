//! Read port for the post-game survey (ADR-0038).
//!
//! Separate from `GameHistory` on purpose, the same way `RatingStore` is
//! (ADR-0034): history is append-only and fire-and-forget behind a writer
//! thread that owns its connection (ADR-0005), and `MemoryHistory` cannot
//! answer a relational query at all. This port only reads, and its `SQLite`
//! adapter proves it - the connection is opened read-only.

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;

/// Ceiling on rows returned in one read. The console filters and sorts the
/// whole set in the page (ADR-0038: no pagination), so this is the point
/// where "see everything at once" stops being true.
pub const FEEDBACK_QUERY_LIMIT: usize = 500;

/// How the answer's author placed in the game they rated. A 2/5 from
/// someone who lost a long game is a different datum from a 2/5 from a
/// winner, so the outcome travels with the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Won,
    Lost,
    /// No winner was recorded - the game never reached an end event (the
    /// process died mid-game, or the room dissolved).
    Unknown,
}

/// One survey answer, joined to the game that produced it.
#[derive(Debug, Clone, Serialize)]
pub struct FeedbackEntry {
    /// 1-5. The room validated the range before storing it.
    pub rating: u8,
    pub comment: Option<String>,
    /// Unix seconds the answer was recorded.
    pub at: i64,
    pub room: String,
    /// Wall-clock length of the rated game, in seconds. `None` when the
    /// game recorded no end.
    pub duration_secs: Option<i64>,
    /// Seats at the table when the game started (bots included: they are
    /// seats, and they change how the game plays).
    pub seats: usize,
    pub outcome: Outcome,
    /// The author's identity: the token `sub`, or a guest's chosen name.
    /// Administrators only (ADR-0038); the console truncates it.
    pub author: String,
}

pub trait FeedbackQuery: Send + Sync {
    /// The `limit` most recent answers, newest first.
    ///
    /// # Errors
    /// When the database cannot be read. Surfaced to the console rather
    /// than swallowed: an empty list must only ever mean "no answers yet".
    fn recent(&self, limit: usize) -> Result<Vec<FeedbackEntry>, String>;
}

/// Read-only `SQLite` adapter over the `--history` database.
///
/// Opens its own connection, like `SqliteRatings`; both run WAL, so this
/// never blocks the history writer thread. `SQLITE_OPEN_READ_ONLY` is the
/// enforcement of "the console cannot write": it is a property of the
/// handle, not a promise made by the query strings.
pub struct SqliteFeedback {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteFeedback {
    /// # Errors
    /// When the database file cannot be opened read-only - which includes
    /// the file not existing yet. Callers open it after `SqliteHistory`,
    /// which creates the file and the schema.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

/// Seat count from the `game.players` column (a JSON array of `PlayerId`).
/// A row we cannot parse still deserves to be shown, so this degrades to 0
/// rather than dropping the answer.
fn seat_count(players: &str) -> usize {
    serde_json::from_str::<Vec<String>>(players).map_or(0, |p| p.len())
}

fn outcome_for(winner: Option<&str>, author: &str) -> Outcome {
    match winner {
        Some(w) if w == author => Outcome::Won,
        Some(_) => Outcome::Lost,
        None => Outcome::Unknown,
    }
}

/// The read itself, taking a borrowed connection so the caller's mutex
/// guard has an explicit, visible lifetime.
fn read_recent(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<FeedbackEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT f.rating, f.comment, f.at, f.player,
                g.room, g.players, g.started_at, g.ended_at, g.winner
         FROM feedback f
         JOIN game g ON g.id = f.game_id
         ORDER BY f.at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
        let rating: i64 = r.get(0)?;
        let author: String = r.get(3)?;
        let players: String = r.get(5)?;
        let started_at: i64 = r.get(6)?;
        let ended_at: Option<i64> = r.get(7)?;
        let winner: Option<String> = r.get(8)?;
        Ok(FeedbackEntry {
            rating: u8::try_from(rating).unwrap_or(0),
            comment: r.get(1)?,
            at: r.get(2)?,
            room: r.get(4)?,
            duration_secs: ended_at.map(|end| end - started_at),
            seats: seat_count(&players),
            outcome: outcome_for(winner.as_deref(), &author),
            author,
        })
    })?;
    rows.collect()
}

impl FeedbackQuery for SqliteFeedback {
    fn recent(&self, limit: usize) -> Result<Vec<FeedbackEntry>, String> {
        let guard = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let entries = read_recent(&guard, limit).map_err(|e| e.to_string());
        drop(guard);
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{GameHistory, SqliteHistory};

    /// Writes one finished game with two answers, then reads them back
    /// through the real SQL - the join, the derived duration and the
    /// won/lost derivation are the whole point of this port.
    #[test]
    fn reads_answers_joined_to_their_game() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("history.db");

        let history = SqliteHistory::open(&path).expect("open history");
        history.record_start("ABCDE", &["alice".into(), "bob".into()], 42);
        history.record_end("ABCDE", Some("alice"));
        history.record_feedback("ABCDE", "alice", 5, Some("tight game"));
        history.record_feedback("ABCDE", "bob", 2, None);
        // Drop drains the writer queue, so every row is committed below.
        drop(history);

        let query = SqliteFeedback::open(&path).expect("open query");
        let entries = query.recent(FEEDBACK_QUERY_LIMIT).expect("read");

        assert_eq!(entries.len(), 2);
        let alice = entries
            .iter()
            .find(|e| e.author == "alice")
            .expect("alice's answer");
        assert_eq!(alice.rating, 5);
        assert_eq!(alice.comment.as_deref(), Some("tight game"));
        assert_eq!(alice.outcome, Outcome::Won);
        assert_eq!(alice.seats, 2);
        assert_eq!(alice.room, "ABCDE");
        assert!(alice.duration_secs.is_some());

        let bob = entries
            .iter()
            .find(|e| e.author == "bob")
            .expect("bob's answer");
        assert_eq!(bob.outcome, Outcome::Lost);
        assert_eq!(bob.comment, None);
    }

    /// A game with no recorded end must not read as a loss for everyone.
    #[test]
    fn unfinished_game_has_unknown_outcome_and_no_duration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("history.db");

        let history = SqliteHistory::open(&path).expect("open history");
        history.record_start("VUKAP", &["alice".into()], 1);
        history.record_feedback("VUKAP", "alice", 3, None);
        drop(history);

        let entries = SqliteFeedback::open(&path)
            .expect("open query")
            .recent(FEEDBACK_QUERY_LIMIT)
            .expect("read");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].outcome, Outcome::Unknown);
        assert_eq!(entries[0].duration_secs, None);
    }

    #[test]
    fn the_connection_refuses_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("history.db");
        drop(SqliteHistory::open(&path).expect("open history"));

        let query = SqliteFeedback::open(&path).expect("open query");
        let write = query
            .conn
            .lock()
            .expect("lock")
            .execute("DELETE FROM feedback", []);
        assert!(write.is_err(), "read-only handle must reject a write");
    }

    #[test]
    fn unparsable_player_list_still_yields_the_answer() {
        assert_eq!(seat_count("not json"), 0);
        assert_eq!(seat_count(r#"["a","b","c"]"#), 3);
    }
}
