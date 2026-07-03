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
    TurnPhase,
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
    turn_timeout: Option<Duration>,
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
        turn_timeout,
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
    /// `Some(d)`: after `d` without game progress the room plays the
    /// canonical action for the acting player (AFK protection).
    turn_timeout: Option<Duration>,
}

impl Room {
    async fn run(mut self, mut rx: mpsc::Receiver<RoomCmd>, rooms: Rooms) {
        info!(room = %self.code, "room created");
        let mut last_activity = tokio::time::Instant::now();
        let mut turn_deadline = tokio::time::Instant::now();
        loop {
            let idle = tokio::time::sleep_until(last_activity + IDLE_TIMEOUT);
            let afk_armed = self.turn_timeout.is_some() && matches!(self.phase, Phase::Active(_));
            let afk = tokio::time::sleep_until(turn_deadline);
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    last_activity = tokio::time::Instant::now();
                    // ponytail: any accepted command resets the turn clock;
                    // per-actor deadlines if trade-offer spam ever stalls games.
                    let advanced = match cmd {
                        RoomCmd::Join { identity, tx, reply } => {
                            let result = self.handle_join(identity, tx);
                            let _ = reply.send(result);
                            false
                        }
                        RoomCmd::Disconnect { player_id } => {
                            if self.handle_disconnect(&player_id) {
                                break; // Empty lobby: dissolve the room.
                            }
                            false
                        }
                        RoomCmd::Start { player_id } => self.handle_start(&player_id),
                        RoomCmd::Game { player_id, cmd } => self.handle_game(&player_id, cmd),
                    };
                    if advanced {
                        if let Some(t) = self.turn_timeout {
                            turn_deadline = tokio::time::Instant::now() + t;
                        }
                    }
                }
                _ = afk, if afk_armed => {
                    if let Some((player, kind)) = self.afk_command() {
                        info!(room = %self.code, player = %player, action = ?kind,
                              "turn timeout, playing canonical action");
                        self.handle_game(&player, kind);
                    }
                    turn_deadline = tokio::time::Instant::now()
                        + self.turn_timeout.expect("armed implies Some");
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

    /// The action the game is waiting for, per `TurnPhase` (the same mapping
    /// the deterministic-replay test uses). Never invalid for the returned
    /// player, so applying it always advances a stalled game.
    fn afk_command(&self) -> Option<(PlayerId, CommandKind)> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        let (seat, kind) = match st.turn {
            TurnPhase::AwaitRoll => (st.current, CommandKind::Roll),
            TurnPhase::AwaitBuy { .. } => (st.current, CommandKind::Decline),
            TurnPhase::AwaitEnd => (st.current, CommandKind::EndTurn),
            TurnPhase::Auction { turn, .. } => (turn, CommandKind::Pass),
        };
        Some((st.players[seat].id.clone(), kind))
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
            Phase::Active(state) | Phase::Finished(state) => Some(Box::new(ClientView::of(state))),
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

    /// Returns true when the game actually started (turn clock should reset).
    fn handle_start(&mut self, player_id: &str) -> bool {
        if !matches!(self.phase, Phase::Lobby) {
            self.send_error(player_id, "game already started");
            return false;
        }
        let is_host = self
            .seats
            .first()
            .is_some_and(|s| s.identity.player_id == player_id);
        if !is_host {
            self.send_error(player_id, "only the host can start the game");
            return false;
        }
        if self.seats.len() < MIN_PLAYERS {
            self.send_error(player_id, "need at least 2 players");
            return false;
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
        true
    }

    /// Returns true when the command was accepted (turn clock should reset).
    fn handle_game(&mut self, player_id: &str, kind: CommandKind) -> bool {
        let state = match &self.phase {
            Phase::Active(state) => state,
            Phase::Lobby => {
                self.send_error(player_id, "game not started");
                return false;
            }
            Phase::Finished(_) => {
                self.send_rejection(player_id, CommandError::GameFinished);
                return false;
            }
        };

        let cmd = PlayerCommand {
            player: player_id.to_string(),
            kind,
        };
        match self.engine.apply(state, &cmd) {
            Err(e) => {
                self.send_rejection(player_id, e);
                false
            }
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
                    let winner_id = self
                        .seats
                        .get(winner)
                        .map(|s| s.identity.player_id.as_str());
                    self.history.record_end(&self.code, winner_id);
                    info!(room = %self.code, winner = ?winner_id, "game finished");
                }
                true
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::MemoryHistory;
    use parcello_engine::Event;
    use std::path::Path;

    /// With a paused clock and zero player input, the room task must play
    /// canonical actions on its own once the per-turn deadline passes.
    #[tokio::test(start_paused = true)]
    async fn afk_timer_plays_canonical_actions() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(&rooms, content, history, Some(Duration::from_secs(30)))
            .await
            .expect("room created");
        let room = rooms.read().await.get(&code).cloned().expect("room handle");

        let mut client_rxs = Vec::new();
        for name in ["alice", "bob"] {
            let (tx, rx) = mpsc::unbounded_channel();
            let (reply, joined) = oneshot::channel();
            room.send(RoomCmd::Join {
                identity: Identity {
                    player_id: format!("guest:{name}"),
                    name: name.to_string(),
                },
                tx,
                reply,
            })
            .await
            .expect("room task alive");
            joined.await.expect("reply sent").expect("join accepted");
            client_rxs.push(rx);
        }
        room.send(RoomCmd::Start {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");

        let auto_rolled = tokio::time::timeout(Duration::from_secs(300), async {
            while let Some(msg) = client_rxs[1].recv().await {
                if let ServerMessage::Update { events, .. } = msg {
                    if events.iter().any(|e| matches!(e, Event::DiceRolled { .. })) {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .expect("an update must arrive before the mock-clock timeout");
        assert!(auto_rolled, "AFK timer should roll for the idle player");
    }
}
