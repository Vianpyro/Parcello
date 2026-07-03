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
    ClientView, CommandError, CommandKind, Engine, Event, GamePhase, GameState, PlayerCommand,
    PlayerId, TurnPhase,
};
use parcello_mods::ResolvedContent;
use parcello_protocol::{SeatInfo, ServerMessage};
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{error, info, warn};

use crate::auth::Identity;
use crate::history::GameHistory;

pub const MIN_PLAYERS: usize = 2;
pub const MAX_PLAYERS: usize = 6;
/// A room with no connected seats for this long dissolves itself. Rejoin is
/// impossible afterwards; the seed and command log make replays external.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// A disconnected player's turn is auto-played after this grace even when
/// `--turn-timeout` is off: someone who left must never stall the table.
/// The grace lets a brief network blip recover (they keep their seat and
/// its reconnect token, ADR-0008).
// ponytail: fixed 30s; make it a flag only if operators ask to tune it.
pub const DISCONNECTED_GRACE: Duration = Duration::from_secs(30);

pub type Rooms = Arc<RwLock<HashMap<String, mpsc::Sender<RoomCmd>>>>;
pub type ClientTx = mpsc::UnboundedSender<ServerMessage>;

pub enum RoomCmd {
    Join {
        identity: Identity,
        /// Reconnect token presented by the client (ADR-0008); required to
        /// re-take a seat held by a spoofable identity.
        reconnect: Option<String>,
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
    /// Post-game survey answer; validated and deduped here.
    Feedback {
        player_id: PlayerId,
        rating: u8,
        comment: Option<String>,
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

/// Trade lifecycle events are private to their two parties (ADR-0007);
/// everything else is public.
fn event_visible_to(event: &Event, seat: usize) -> bool {
    match *event {
        Event::TradeProposed { from, to, .. }
        | Event::TradeAccepted { from, to, .. }
        | Event::TradeDeclined { from, to, .. }
        | Event::TradeCancelled { from, to, .. } => seat == from || seat == to,
        _ => true,
    }
}

fn random_code() -> String {
    (0..5)
        .map(|_| rand::random_range(b'A'..=b'Z') as char)
        .collect()
}

/// 32 alphanumeric chars from the thread CSPRNG (~190 bits): unguessable
/// proof of seat ownership for the room's lifetime (ADR-0008).
fn new_reconnect_token() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    (0..32)
        .map(|_| CHARS[rand::random_range(0..CHARS.len())] as char)
        .collect()
}

/// Constant-time-ish comparison: no early exit on the first differing byte.
fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

struct Seat {
    identity: Identity,
    /// `None` while disconnected; the seat survives for rejoin.
    tx: Option<ClientTx>,
    /// Issued in `Joined`; proves seat ownership on rejoin (ADR-0008).
    reconnect: String,
    /// One post-game survey answer per seat.
    feedback_given: bool,
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
    /// `Some(d)`: a connected but idle player's turn is auto-played after
    /// `d` (AFK protection). Disconnected players are always skipped after
    /// `DISCONNECTED_GRACE`, independent of this.
    turn_timeout: Option<Duration>,
}

impl Room {
    async fn run(mut self, mut rx: mpsc::Receiver<RoomCmd>, rooms: Rooms) {
        info!(room = %self.code, "room created");
        let mut last_activity = tokio::time::Instant::now();
        let mut last_progress = tokio::time::Instant::now();
        loop {
            let idle = tokio::time::sleep_until(last_activity + IDLE_TIMEOUT);
            // Smart per-turn deadline, recomputed each loop so a mid-turn
            // disconnect shortens it: disconnected acting seats are skipped
            // after a short grace (always on); a connected but slow player
            // gets the configured --turn-timeout, or unlimited time when off.
            let deadline = self.afk_deadline();
            let afk_armed = deadline.is_some();
            let afk = tokio::time::sleep_until(last_progress + deadline.unwrap_or(IDLE_TIMEOUT));
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    last_activity = tokio::time::Instant::now();
                    let advanced = match cmd {
                        RoomCmd::Join { identity, reconnect, tx, reply } => {
                            let result = self.handle_join(identity, reconnect, tx);
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
                        RoomCmd::Feedback { player_id, rating, comment } => {
                            self.handle_feedback(&player_id, rating, comment);
                            false
                        }
                    };
                    // Any accepted command resets the turn clock.
                    if advanced {
                        last_progress = tokio::time::Instant::now();
                    }
                }
                _ = afk, if afk_armed => {
                    if let Some((player, kind)) = self.afk_command() {
                        info!(room = %self.code, player = %player, action = ?kind,
                              "turn timeout, playing canonical action");
                        self.handle_game(&player, kind);
                    }
                    last_progress = tokio::time::Instant::now();
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

    /// Seat expected to act right now: the auction bidder, else the current
    /// player. `None` outside an active game.
    fn acting_seat(&self) -> Option<usize> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        Some(match st.turn {
            TurnPhase::Auction { turn, .. } => turn,
            _ => st.current,
        })
    }

    /// How long the acting seat may stall before its canonical action is
    /// auto-played, or `None` for no limit. A disconnected seat (truly AFK)
    /// is skipped after `DISCONNECTED_GRACE` whether or not `--turn-timeout`
    /// is set; a connected but slow player only faces `--turn-timeout`.
    fn afk_deadline(&self) -> Option<Duration> {
        let seat = self.acting_seat()?;
        let connected = self.seats.get(seat).is_some_and(|s| s.tx.is_some());
        if connected {
            self.turn_timeout
        } else {
            Some(
                self.turn_timeout
                    .map_or(DISCONNECTED_GRACE, |t| t.min(DISCONNECTED_GRACE)),
            )
        }
    }

    /// The action the game is waiting for, per `TurnPhase` (the same mapping
    /// the deterministic-replay test uses). Never invalid for the returned
    /// player, so applying it always advances a stalled game.
    fn afk_command(&self) -> Option<(PlayerId, CommandKind)> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        let seat = self.acting_seat()?;
        let kind = match st.turn {
            TurnPhase::AwaitRoll => CommandKind::Roll,
            TurnPhase::AwaitBuy { .. } => CommandKind::Decline,
            TurnPhase::AwaitEnd => CommandKind::EndTurn,
            TurnPhase::Auction { .. } => CommandKind::Pass,
        };
        Some((st.players[seat].id.clone(), kind))
    }

    fn handle_join(
        &mut self,
        identity: Identity,
        reconnect: Option<String>,
        tx: ClientTx,
    ) -> Result<(), String> {
        let seat_index = match self.seat_of(&identity.player_id) {
            Some(i) => {
                // Rejoin: last connection wins, but a spoofable (guest)
                // identity must prove seat ownership with the reconnect
                // token issued at first join (ADR-0008). JWT identities
                // are cryptographically bound and need no token.
                let proven = !self.seats[i].identity.spoofable
                    || reconnect
                        .as_deref()
                        .is_some_and(|t| token_eq(t, &self.seats[i].reconnect));
                if !proven {
                    return Err("seat is protected: rejoin with its reconnect token".into());
                }
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
                    reconnect: new_reconnect_token(),
                    feedback_given: false,
                });
                self.seats.len() - 1
            }
        };

        let view = match &self.phase {
            Phase::Lobby => None,
            Phase::Active(state) | Phase::Finished(state) => {
                Some(Box::new(ClientView::for_seat(state, seat_index)))
            }
        };
        let joined = ServerMessage::Joined {
            code: self.code.clone(),
            seat: seat_index,
            players: self.seat_infos(),
            content: Box::new((*self.content).clone()),
            view,
            reconnect: Some(self.seats[seat_index].reconnect.clone()),
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

        let msgs: Vec<ServerMessage> = (0..self.seats.len())
            .map(|seat| ServerMessage::GameStarted {
                view: Box::new(ClientView::for_seat(&state, seat)),
            })
            .collect();
        self.phase = Phase::Active(state);
        self.send_per_seat(msgs);
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
                // One view + event feed per seat: trade offers and their
                // lifecycle events reach only the two parties (ADR-0007).
                let msgs: Vec<ServerMessage> = (0..self.seats.len())
                    .map(|seat| ServerMessage::Update {
                        events: events
                            .iter()
                            .filter(|e| event_visible_to(e, seat))
                            .cloned()
                            .collect(),
                        view: Box::new(ClientView::for_seat(&next, seat)),
                    })
                    .collect();
                self.phase = match finished_winner {
                    Some(_) => Phase::Finished(next),
                    None => Phase::Active(next),
                };
                self.send_per_seat(msgs);
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

    /// Post-game survey (Fortnite-style, but opt-in and non-blocking): only
    /// once the game is over, once per seat, rating 1-5, comment capped.
    fn handle_feedback(&mut self, player_id: &str, rating: u8, comment: Option<String>) {
        if !matches!(self.phase, Phase::Finished(_)) {
            return self.send_error(player_id, "feedback opens when the game ends");
        }
        let Some(seat) = self.seat_of(player_id) else {
            return;
        };
        if !(1..=5).contains(&rating) {
            return self.send_error(player_id, "rating must be 1-5");
        }
        if self.seats[seat].feedback_given {
            return self.send_error(player_id, "feedback already recorded");
        }
        let comment = comment
            .map(|c| c.trim().chars().take(500).collect::<String>())
            .filter(|c| !c.is_empty());
        self.seats[seat].feedback_given = true;
        self.history
            .record_feedback(&self.code, player_id, rating, comment.as_deref());
        info!(room = %self.code, player = %player_id, rating, "feedback recorded");
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
            if let Some(tx) = &seat.tx
                && tx.send(msg.clone()).is_err()
            {
                seat.tx = None; // Connection gone; seat stays for rejoin.
            }
        }
    }

    /// Delivers `msgs[i]` to seat `i` (per-seat views, ADR-0007).
    fn send_per_seat(&mut self, msgs: Vec<ServerMessage>) {
        for (seat, msg) in msgs.into_iter().enumerate() {
            if let Some(tx) = &self.seats[seat].tx
                && tx.send(msg).is_err()
            {
                self.seats[seat].tx = None;
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

    /// Post-game survey rules: rejected while the game runs, accepted once
    /// finished, stored in history, one per seat.
    #[tokio::test]
    async fn feedback_only_after_game_end_and_once_per_seat() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let memory = Arc::new(MemoryHistory::new());
        let history: Arc<dyn GameHistory> = memory.clone();
        let code = create_room(&rooms, content, history, None)
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
                    spoofable: true,
                },
                reconnect: None,
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

        let feedback = |rating: u8| RoomCmd::Feedback {
            player_id: "guest:alice".into(),
            rating,
            comment: Some("  great game  ".into()),
        };
        // Mid-game: rejected.
        room.send(feedback(5)).await.expect("room task alive");
        // Bob resigns: alice wins, the game is finished.
        room.send(RoomCmd::Game {
            player_id: "guest:bob".into(),
            cmd: CommandKind::Resign,
        })
        .await
        .expect("room task alive");
        room.send(feedback(0)).await.expect("room task alive"); // invalid rating
        room.send(feedback(5)).await.expect("room task alive"); // accepted
        room.send(feedback(3)).await.expect("room task alive"); // duplicate

        tokio::time::sleep(Duration::from_millis(200)).await;
        let recorded: Vec<String> = memory
            .dump(&code)
            .into_iter()
            .filter(|l| l.starts_with("feedback"))
            .collect();
        assert_eq!(
            recorded,
            vec![r#"feedback player=guest:alice rating=5 comment=Some("great game")"#],
            "exactly one survey answer must be recorded, trimmed"
        );
        let errors = |rx: &mut mpsc::UnboundedReceiver<ServerMessage>| {
            let mut n = 0;
            while let Ok(msg) = rx.try_recv() {
                if matches!(msg, ServerMessage::Error { .. }) {
                    n += 1;
                }
            }
            n
        };
        assert_eq!(
            errors(&mut client_rxs[0]),
            3,
            "mid-game, invalid rating, and duplicate must each error"
        );
    }

    /// A guest seat can only be re-taken with the reconnect token issued at
    /// first join (ADR-0008): knowing the name is no longer enough.
    #[tokio::test]
    async fn guest_seat_rejoin_requires_the_reconnect_token() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(&rooms, content, history, None)
            .await
            .expect("room created");
        let room = rooms.read().await.get(&code).cloned().expect("room handle");

        let alice = || Identity {
            player_id: "guest:alice".to_string(),
            name: "alice".to_string(),
            spoofable: true,
        };
        let join = |reconnect: Option<String>| {
            let room = room.clone();
            let identity = alice();
            async move {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let (reply, joined) = oneshot::channel();
                room.send(RoomCmd::Join {
                    identity,
                    reconnect,
                    tx,
                    reply,
                })
                .await
                .expect("room task alive");
                let result = joined.await.expect("reply sent");
                let token = match rx.try_recv() {
                    Ok(ServerMessage::Joined { reconnect, .. }) => reconnect,
                    _ => None,
                };
                (result, token)
            }
        };

        let (result, token) = join(None).await;
        result.expect("first join accepted");
        let token = token.expect("token issued on join");

        let (hijack, _) = join(None).await;
        assert!(hijack.is_err(), "name alone must not re-take the seat");
        let (bad, _) = join(Some("wrong-token".into())).await;
        assert!(bad.is_err(), "wrong token must not re-take the seat");
        let (rejoin, _) = join(Some(token)).await;
        rejoin.expect("correct token re-takes the seat");
    }

    /// A third party must receive neither the trade event nor the offer in
    /// its view (ADR-0007): per-seat routing through the real room task.
    #[tokio::test]
    async fn trade_offers_are_invisible_to_third_parties() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(&rooms, content, history, None)
            .await
            .expect("room created");
        let room = rooms.read().await.get(&code).cloned().expect("room handle");

        let mut client_rxs = Vec::new();
        for name in ["alice", "bob", "carol"] {
            let (tx, rx) = mpsc::unbounded_channel();
            let (reply, joined) = oneshot::channel();
            room.send(RoomCmd::Join {
                identity: Identity {
                    player_id: format!("guest:{name}"),
                    name: name.to_string(),
                    spoofable: true,
                },
                reconnect: None,
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
        room.send(RoomCmd::Game {
            player_id: "guest:alice".into(),
            cmd: CommandKind::ProposeTrade {
                to: "guest:bob".into(),
                give_cash: 50,
                give_tiles: vec![],
                receive_cash: 0,
                receive_tiles: vec![],
            },
        })
        .await
        .expect("room task alive");

        // First Update after GameStarted is the accepted trade proposal.
        let mut update_for = |seat: usize| {
            let rx = &mut client_rxs[seat];
            loop {
                match rx.try_recv() {
                    Ok(ServerMessage::Update { events, view }) => break (events, view),
                    Ok(_) => continue,
                    Err(_) => panic!("seat {seat} never received an Update"),
                }
            }
        };
        // Give the room task time to process both commands.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let (events, view) = update_for(1);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TradeProposed { .. })),
            "recipient must see the proposal"
        );
        assert_eq!(view.pending_trades.len(), 1);

        let (events, view) = update_for(2);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::TradeProposed { .. })),
            "third party must not see the trade event"
        );
        assert!(
            view.pending_trades.is_empty(),
            "third party must not see the offer"
        );
    }

    /// A disconnected player's turn is auto-played after the grace period
    /// even with no `--turn-timeout` set, so an AFK player never stalls the
    /// table. (We assert alice's roll is auto-played; whether her turn then
    /// fully hands off depends on the game - an auction, say, legitimately
    /// waits on the still-connected bob.)
    #[tokio::test(start_paused = true)]
    async fn disconnected_player_is_skipped_after_grace() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        // No --turn-timeout: the smart grace alone must skip the AFK seat.
        let code = create_room(&rooms, content, history, None)
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
                    spoofable: true,
                },
                reconnect: None,
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
        // Alice (seat 0) is the acting player and drops off.
        room.send(RoomCmd::Disconnect {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");

        // With no turn timeout, the grace alone must auto-play alice's roll;
        // bob sees it without sending anything.
        let auto_rolled = tokio::time::timeout(Duration::from_secs(300), async {
            while let Some(msg) = client_rxs[1].recv().await {
                if let ServerMessage::Update { events, .. } = msg
                    && events.iter().any(|e| matches!(e, Event::DiceRolled { .. }))
                {
                    return true;
                }
            }
            false
        })
        .await
        .expect("an update must arrive before the mock-clock timeout");
        assert!(
            auto_rolled,
            "the disconnected player's turn must be auto-played after the grace"
        );
    }

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
                    spoofable: true,
                },
                reconnect: None,
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
                if let ServerMessage::Update { events, .. } = msg
                    && events.iter().any(|e| matches!(e, Event::DiceRolled { .. }))
                {
                    return true;
                }
            }
            false
        })
        .await
        .expect("an update must arrive before the mock-clock timeout");
        assert!(auto_rolled, "AFK timer should roll for the idle player");
    }
}
