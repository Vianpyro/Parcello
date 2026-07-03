//! Room lifecycle: one dedicated Tokio task per room (Session Layer).
//!
//! Room state machine: Lobby -> Active -> Finished. The architecture's
//! `Starting` state collapses to a point here because mods are resolved
//! server-wide at boot (ADR-0004), so there is nothing to await per room.
//!
//! The room task owns the `Engine` and the authoritative `GameState`.
//! Connections talk to it exclusively through `RoomCmd` messages; replies and
//! broadcasts flow back through per-connection unbounded senders.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parcello_engine::{
    ClientView, CommandError, CommandKind, Engine, GamePhase, GameState, PlayerCommand, PlayerId,
};
use parcello_mods::ResolvedContent;
use parcello_protocol::{SeatInfo, ServerMessage};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{error, info, warn};

use crate::auth::Identity;
use crate::history::GameHistory;

pub const MIN_PLAYERS: usize = 2;
pub const MAX_PLAYERS: usize = 6;
/// A room with no connected seats for this long dissolves itself. Rejoin is
/// impossible afterwards; the seed and command log make replays external.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

pub type Rooms = Arc<RwLock<HashMap<String, mpsc::Sender<RoomCmd>>>>;
pub type ClientTx = mpsc::UnboundedSender<ServerMessage>;

pub enum RoomCmd {
    Join {
        identity: Identity,
        tx: ClientTx,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Disconnect {
        player_id: PlayerId,
    },
    Start {
        player_id: PlayerId,
    },
    Game {
        player_id: PlayerId,
        cmd: CommandKind,
    },
}

/// Creates the room task and registers its handle. Returns the room code.
pub async fn create_room(
    rooms: &Rooms,
    content: Arc<ResolvedContent>,
    history: Arc<dyn GameHistory>,
) -> Result<String, String> {
    let engine = Engine::new(Arc::new(content.content.clone()))
        .map_err(|e| format!("invalid room content: {e}"))?;
    let (tx, rx) = mpsc::channel(64);

    let mut registry = rooms.write().await;
    let code = loop {
        let candidate = random_code();
        if !registry.contains_key(&candidate) {
            break candidate;
        }
    };
    registry.insert(code.clone(), tx);
    drop(registry);

    let room = Room {
        code: code.clone(),
        content,
        engine,
        seats: Vec::new(),
        phase: Phase::Lobby,
        history,
    };
    tokio::spawn(room.run(rx, Arc::clone(rooms)));
    Ok(code)
}

fn random_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..5).map(|_| rng.gen_range(b'A'..=b'Z') as char).collect()
}

struct Seat {
    identity: Identity,
    /// `None` while disconnected; the seat survives for rejoin.
    tx: Option<ClientTx>,
}

enum Phase {
    Lobby,
    Active(GameState),
    Finished(GameState),
}

struct Room {
    code: String,
    content: Arc<ResolvedContent>,
    engine: Engine,
    seats: Vec<Seat>,
    phase: Phase,
    history: Arc<dyn GameHistory>,
}

impl Room {
    async fn run(mut self, mut rx: mpsc::Receiver<RoomCmd>, rooms: Rooms) {
        info!(room = %self.code, "room created");
        let mut last_activity = tokio::time::Instant::now();
        loop {
            let idle = tokio::time::sleep_until(last_activity + IDLE_TIMEOUT);
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    last_activity = tokio::time::Instant::now();
                    match cmd {
                        RoomCmd::Join { identity, tx, reply } => {
                            let result = self.handle_join(identity, tx);
                            let _ = reply.send(result);
                        }
                        RoomCmd::Disconnect { player_id } => {
                            if self.handle_disconnect(&player_id) {
                                break; // Empty lobby: dissolve the room.
                            }
                        }
                        RoomCmd::Start { player_id } => self.handle_start(&player_id),
                        RoomCmd::Game { player_id, cmd } => self.handle_game(&player_id, cmd),
                    }
                }
                _ = idle => {
                    if self.seats.iter().all(|s| s.tx.is_none()) {
                        info!(room = %self.code, "idle timeout, dissolving");
                        break;
                    }
                    last_activity = tokio::time::Instant::now();
                }
            }
        }
        rooms.write().await.remove(&self.code);
        info!(room = %self.code, "room dissolved");
    }

    fn handle_join(&mut self, identity: Identity, tx: ClientTx) -> Result<(), String> {
        let seat_index = match self.seat_of(&identity.player_id) {
            Some(i) => {
                // Rejoin: last connection wins; the seat is preserved.
                self.seats[i].tx = Some(tx.clone());
                info!(room = %self.code, player = %identity.player_id, seat = i, "rejoined");
                i
            }
            None => {
                if !matches!(self.phase, Phase::Lobby) {
                    return Err("game already started".into());
                }
                if self.seats.len() >= MAX_PLAYERS {
                    return Err("room is full".into());
                }
                self.seats.push(Seat {
                    identity,
                    tx: Some(tx.clone()),
                });
                self.seats.len() - 1
            }
        };

        let view = match &self.phase {
            Phase::Lobby => None,
            Phase::Active(state) | Phase::Finished(state) => {
                Some(Box::new(ClientView::of(state)))
            }
        };
        let joined = ServerMessage::Joined {
            code: self.code.clone(),
            seat: seat_index,
            players: self.seat_infos(),
            content: Box::new((*self.content).clone()),
            view,
        };
        if tx.send(joined).is_err() {
            self.seats[seat_index].tx = None;
        }
        self.broadcast_lobby();
        Ok(())
    }

    /// Returns true when the room should dissolve (lobby emptied out).
    fn handle_disconnect(&mut self, player_id: &str) -> bool {
        match self.phase {
            Phase::Lobby => {
                // Free the seat entirely; host role follows seat 0.
                self.seats.retain(|s| s.identity.player_id != player_id);
                self.broadcast_lobby();
                self.seats.is_empty()
            }
            Phase::Active(_) | Phase::Finished(_) => {
                if let Some(i) = self.seat_of(player_id) {
                    self.seats[i].tx = None;
                }
                self.broadcast_lobby();
                false
            }
        }
    }

    fn handle_start(&mut self, player_id: &str) {
        if !matches!(self.phase, Phase::Lobby) {
            return self.send_error(player_id, "game already started");
        }
        let is_host = self
            .seats
            .first()
            .is_some_and(|s| s.identity.player_id == player_id);
        if !is_host {
            return self.send_error(player_id, "only the host can start the game");
        }
        if self.seats.len() < MIN_PLAYERS {
            return self.send_error(player_id, "need at least 2 players");
        }

        let players: Vec<(PlayerId, String)> = self
            .seats
            .iter()
            .map(|s| (s.identity.player_id.clone(), s.identity.name.clone()))
            .collect();
        let seed: u64 = rand::random();
        let state = self.engine.new_game(players.clone(), seed);
        self.history.record_start(
            &self.code,
            &players.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            seed,
        );
        info!(room = %self.code, players = self.seats.len(), "game started");

        let view = Box::new(ClientView::of(&state));
        self.phase = Phase::Active(state);
        self.broadcast(ServerMessage::GameStarted { view });
    }

    fn handle_game(&mut self, player_id: &str, kind: CommandKind) {
        let state = match &self.phase {
            Phase::Active(state) => state,
            Phase::Lobby => return self.send_error(player_id, "game not started"),
            Phase::Finished(_) => {
                return self.send_rejection(player_id, CommandError::GameFinished)
            }
        };

        let cmd = PlayerCommand {
            player: player_id.to_string(),
            kind,
        };
        match self.engine.apply(state, &cmd) {
            Err(e) => self.send_rejection(player_id, e),
            Ok((next, events)) => {
                self.history.record_command(&self.code, &cmd);
                let finished_winner = match next.phase {
                    GamePhase::Finished { winner } => Some(winner),
                    GamePhase::Active => None,
                };
                let view = Box::new(ClientView::of(&next));
                self.phase = match finished_winner {
                    Some(_) => Phase::Finished(next),
                    None => Phase::Active(next),
                };
                self.broadcast(ServerMessage::Update { events, view });
                if let Some(winner) = finished_winner {
                    let winner_id = self.seats.get(winner).map(|s| s.identity.player_id.as_str());
                    self.history.record_end(&self.code, winner_id);
                    info!(room = %self.code, winner = ?winner_id, "game finished");
                }
            }
        }
    }

    fn seat_of(&self, player_id: &str) -> Option<usize> {
        self.seats
            .iter()
            .position(|s| s.identity.player_id == player_id)
    }

    fn seat_infos(&self) -> Vec<SeatInfo> {
        self.seats
            .iter()
            .enumerate()
            .map(|(i, s)| SeatInfo {
                seat: i,
                player_id: s.identity.player_id.clone(),
                name: s.identity.name.clone(),
                connected: s.tx.is_some(),
            })
            .collect()
    }

    fn broadcast_lobby(&mut self) {
        let players = self.seat_infos();
        self.broadcast(ServerMessage::Lobby { players });
    }

    fn broadcast(&mut self, msg: ServerMessage) {
        for seat in &mut self.seats {
            if let Some(tx) = &seat.tx {
                if tx.send(msg.clone()).is_err() {
                    seat.tx = None; // Connection gone; seat stays for rejoin.
                }
            }
        }
    }

    fn send_to(&mut self, player_id: &str, msg: ServerMessage) {
        let Some(i) = self.seat_of(player_id) else {
            warn!(room = %self.code, player = %player_id, "message for unknown seat");
            return;
        };
        if let Some(tx) = &self.seats[i].tx {
            if tx.send(msg).is_err() {
                self.seats[i].tx = None;
            }
        } else {
            error!(room = %self.code, player = %player_id, "send to disconnected seat");
        }
    }

    fn send_error(&mut self, player_id: &str, message: &str) {
        self.send_to(
            player_id,
            ServerMessage::Error {
                message: message.to_string(),
            },
        );
    }

    fn send_rejection(&mut self, player_id: &str, error: CommandError) {
        self.send_to(player_id, ServerMessage::Rejected { error });
    }
}
