//! Persistence port for game history (Repository pattern).
//!
//! Business code depends on this trait only. `MemoryHistory` is the MVP
//! adapter; a SQLx/SQLite adapter replaces it later with no caller changes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use parcello_engine::{PlayerCommand, PlayerId};
use tracing::warn;

pub trait GameHistory: Send + Sync {
    fn record_start(&self, room: &str, players: &[PlayerId], seed: u64);
    /// Called only for accepted commands: the log replays deterministically.
    fn record_command(&self, room: &str, cmd: &PlayerCommand);
    fn record_end(&self, room: &str, winner: Option<&str>);
}

#[derive(Default)]
pub struct MemoryHistory {
    logs: Mutex<HashMap<String, Vec<String>>>,
}

impl MemoryHistory {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)] // Debug/test introspection; the SQLx adapter will query instead.
    pub fn dump(&self, room: &str) -> Vec<String> {
        self.logs
            .lock()
            .expect("history mutex poisoned")
            .get(room)
            .cloned()
            .unwrap_or_default()
    }

    fn push(&self, room: &str, line: String) {
        self.logs
            .lock()
            .expect("history mutex poisoned")
            .entry(room.to_string())
            .or_default()
            .push(line);
    }
}

impl GameHistory for MemoryHistory {
    fn record_start(&self, room: &str, players: &[PlayerId], seed: u64) {
        self.push(room, format!("start seed={seed} players={players:?}"));
    }

    fn record_command(&self, room: &str, cmd: &PlayerCommand) {
        let line = serde_json::to_string(cmd).unwrap_or_else(|_| format!("{cmd:?}"));
        self.push(room, line);
    }

    fn record_end(&self, room: &str, winner: Option<&str>) {
        self.push(room, format!("end winner={winner:?}"));
    }
}

/// SQLite adapter (ADR-0005): a dedicated writer thread owns the connection;
/// the trait methods enqueue and never block on I/O. Best-effort: write
/// failures are logged, the game continues.
///
/// Schema: `game(id, room, seed, players, started_at, winner, ended_at)` and
/// `command(game_id, seq, at, json)`. `(seed, ordered command json)` is a
/// complete deterministic replay (ADR-0001/0002).
pub struct SqliteHistory {
    tx: Option<mpsc::Sender<Rec>>,
    handle: Option<JoinHandle<()>>,
}

enum Rec {
    Start { room: String, players: String, seed: i64, at: i64 },
    Cmd { room: String, json: String, at: i64 },
    End { room: String, winner: Option<String>, at: i64 },
}

impl SqliteHistory {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS game (
                 id         INTEGER PRIMARY KEY,
                 room       TEXT NOT NULL,
                 seed       INTEGER NOT NULL,
                 players    TEXT NOT NULL,
                 started_at INTEGER NOT NULL,
                 winner     TEXT,
                 ended_at   INTEGER
             );
             CREATE TABLE IF NOT EXISTS command (
                 game_id INTEGER NOT NULL REFERENCES game(id),
                 seq     INTEGER NOT NULL,
                 at      INTEGER NOT NULL,
                 json    TEXT NOT NULL,
                 PRIMARY KEY (game_id, seq)
             );",
        )?;
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("parcello-history".into())
            .spawn(move || writer_loop(conn, rx))
            .expect("history thread spawns");
        Ok(Self { tx: Some(tx), handle: Some(handle) })
    }

    fn send(&self, rec: Rec) {
        if let Some(tx) = &self.tx {
            if tx.send(rec).is_err() {
                warn!("history writer thread is gone; record dropped");
            }
        }
    }
}

impl Drop for SqliteHistory {
    fn drop(&mut self) {
        // Close the queue, then drain: pending records land before shutdown.
        drop(self.tx.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn writer_loop(conn: rusqlite::Connection, rx: mpsc::Receiver<Rec>) {
    // Room codes can repeat across restarts, so rooms map to rowids per run.
    let mut games: HashMap<String, (i64, i64)> = HashMap::new(); // room -> (game_id, next_seq)
    while let Ok(rec) = rx.recv() {
        let result = match rec {
            Rec::Start { room, players, seed, at } => conn
                .execute(
                    "INSERT INTO game (room, seed, players, started_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![room, seed, players, at],
                )
                .map(|_| {
                    games.insert(room, (conn.last_insert_rowid(), 0));
                }),
            Rec::Cmd { room, json, at } => match games.get_mut(&room) {
                Some((game_id, seq)) => {
                    *seq += 1;
                    conn.execute(
                        "INSERT INTO command (game_id, seq, at, json) VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![*game_id, *seq, at, json],
                    )
                    .map(|_| ())
                }
                None => {
                    warn!(room, "command for unknown game; dropped");
                    Ok(())
                }
            },
            Rec::End { room, winner, at } => match games.remove(&room) {
                Some((game_id, _)) => conn
                    .execute(
                        "UPDATE game SET winner = ?1, ended_at = ?2 WHERE id = ?3",
                        rusqlite::params![winner, at, game_id],
                    )
                    .map(|_| ()),
                None => Ok(()),
            },
        };
        if let Err(e) = result {
            warn!(error = %e, "history write failed");
        }
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl GameHistory for SqliteHistory {
    fn record_start(&self, room: &str, players: &[PlayerId], seed: u64) {
        let players = serde_json::to_string(players).unwrap_or_else(|_| "[]".into());
        self.send(Rec::Start {
            room: room.to_string(),
            players,
            seed: seed as i64, // bit-preserving; read back with `as u64`
            at: now_unix(),
        });
    }

    fn record_command(&self, room: &str, cmd: &PlayerCommand) {
        let Ok(json) = serde_json::to_string(cmd) else {
            warn!("unserializable command; dropped from history");
            return;
        };
        self.send(Rec::Cmd { room: room.to_string(), json, at: now_unix() });
    }

    fn record_end(&self, room: &str, winner: Option<&str>) {
        self.send(Rec::End {
            room: room.to_string(),
            winner: winner.map(str::to_string),
            at: now_unix(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcello_engine::CommandKind;

    #[test]
    fn sqlite_history_persists_a_replayable_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("history.db");

        let history = SqliteHistory::open(&db).expect("open");
        history.record_start("ABCDE", &["p0".into(), "p1".into()], u64::MAX);
        for kind in [CommandKind::Roll, CommandKind::Buy, CommandKind::EndTurn] {
            history.record_command(
                "ABCDE",
                &PlayerCommand { player: "p0".into(), kind },
            );
        }
        history.record_end("ABCDE", Some("p0"));
        drop(history); // joins the writer: everything is flushed

        let conn = rusqlite::Connection::open(&db).expect("reopen");
        let (seed, players, winner): (i64, String, String) = conn
            .query_row("SELECT seed, players, winner FROM game", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .expect("game row");
        assert_eq!(seed as u64, u64::MAX);
        assert_eq!(players, r#"["p0","p1"]"#);
        assert_eq!(winner, "p0");

        let jsons: Vec<String> = conn
            .prepare("SELECT json FROM command ORDER BY seq")
            .expect("prepare")
            .query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert_eq!(jsons.len(), 3);
        assert!(jsons[0].contains(r#""type":"roll""#));
        assert!(jsons[2].contains(r#""type":"end_turn""#));
    }
}
