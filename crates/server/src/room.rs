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
use parcello_protocol::{RoomSettings, SeatInfo, ServerMessage};
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
/// A bot seat pauses this long before each move so humans can follow the
/// action; without it a table of bots would resolve instantly (ADR-0014).
const BOT_THINK: Duration = Duration::from_millis(800);

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
    PlayAgain {
        player_id: PlayerId,
    },
    /// Host adds a server-driven bot seat (ADR-0014).
    AddBot {
        player_id: PlayerId,
    },
    /// Host drops the most recently added bot seat.
    RemoveBot {
        player_id: PlayerId,
    },
    /// Host replaces the room's settings in the lobby (ADR-0015).
    Configure {
        player_id: PlayerId,
        settings: RoomSettings,
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
    time_bank: Option<Duration>,
    game_timeout: Option<Duration>,
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

    // Initial settings: the mod's rules and the server's default timers.
    // The host tweaks them in the lobby before starting (ADR-0015).
    let settings = RoomSettings {
        game_seconds: game_timeout.map(|d| d.as_secs()),
        turn_seconds: turn_timeout.map(|d| d.as_secs()),
        time_bank_seconds: time_bank.map(|d| d.as_secs()),
        rules: content.content.rules.clone(),
    };
    let room = Room {
        code: code.clone(),
        content,
        engine,
        seats: Vec::new(),
        phase: Phase::Lobby,
        history,
        settings,
        game_deadline: None,
        bot_counter: 0,
        banks: Vec::new(),
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

/// A pronounceable 5-char code (consonant-vowel-consonant-vowel-consonant),
/// easy to read out over voice chat. ~171k combinations - far more than the
/// rooms a self-hosted server holds concurrently, and `create_room` retries
/// on the rare collision.
fn random_code() -> String {
    const CONSONANTS: &[u8] = b"BCDFGHJKLMNPRSTVWXZ";
    const VOWELS: &[u8] = b"AEIOU";
    let pick = |set: &[u8]| set[rand::random_range(0..set.len())] as char;
    [
        pick(CONSONANTS),
        pick(VOWELS),
        pick(CONSONANTS),
        pick(VOWELS),
        pick(CONSONANTS),
    ]
    .iter()
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

/// Clamp host-supplied room settings to sane ranges (ADR-0015). The wire
/// values are untrusted: absurd numbers would break the game (a house level
/// past the 6-entry rent table, negative economy) or the experience (a
/// one-second game). Returned settings are always safe to apply.
fn clamp_settings(mut s: RoomSettings) -> RoomSettings {
    s.game_seconds = s.game_seconds.map(|v| v.clamp(60, 86_400));
    s.turn_seconds = s.turn_seconds.map(|v| v.clamp(5, 3_600));
    s.time_bank_seconds = s.time_bank_seconds.map(|v| v.clamp(0, 600));
    let r = &mut s.rules;
    r.starting_balance = r.starting_balance.clamp(0, 1_000_000);
    r.go_salary = r.go_salary.clamp(0, 100_000);
    r.jail_fine = r.jail_fine.clamp(0, 100_000);
    // rents[] has six levels (0..=5, level 5 = hotel); a higher cap would
    // index past the array in the rent calculator.
    r.max_houses_per_property = r.max_houses_per_property.clamp(1, 5);
    r.bankruptcy_threshold = r.bankruptcy_threshold.clamp(0, 1_000_000);
    r.expropriation = r.expropriation.clamp(0, 1_000);
    r.rent_boost = r.rent_boost.clamp(0, 1_000);
    r.win_full_groups = r.win_full_groups.clamp(0, 100);
    // Small multipliers (base mod uses 6/3), not percents like the rules
    // above - a much smaller ceiling is enough (ADR-0019).
    r.subsidiary_pool_factor = r.subsidiary_pool_factor.clamp(0, 100);
    r.conglomerate_pool_factor = r.conglomerate_pool_factor.clamp(0, 100);
    s
}

struct Seat {
    identity: Identity,
    /// `None` while disconnected; the seat survives for rejoin. Bot seats
    /// keep this `None` for their whole life (nobody connects as a bot).
    tx: Option<ClientTx>,
    /// Issued in `Joined`; proves seat ownership on rejoin (ADR-0008).
    reconnect: String,
    /// One post-game survey answer per seat.
    feedback_given: bool,
    /// A server-driven bot (ADR-0014): the room task plays its turns and it
    /// is evicted to make room for a joining human.
    is_bot: bool,
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
    /// Host-editable per-room config (timers + rules, ADR-0015). Frozen once
    /// the game starts (`Configure` is lobby-only). The turn timer, game
    /// clock, and effective rules are all derived from here at `start_game`.
    settings: RoomSettings,
    /// Absolute instant the game clock fires, set at `start_game` from
    /// `settings.game_seconds`; `None` for an untimed game (ADR-0010).
    game_deadline: Option<tokio::time::Instant>,
    /// Monotonic counter for bot seat names ("Bot 1", "Bot 2", ...), so a
    /// removed-then-readded bot never reuses a `player_id`.
    bot_counter: usize,
    /// Personal time bank remaining per seat, in seconds (ADR-0023). Rebuilt
    /// from `settings.time_bank_seconds` at every `start_game`; never
    /// refilled during a match. Bots never drain it (`Seat.tx` is always
    /// `None` for them, same signal `afk_deadline` uses).
    banks: Vec<u64>,
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
            // Game clock (ADR-0010): fires once at the absolute deadline.
            let game_armed = self.game_deadline.is_some() && matches!(self.phase, Phase::Active(_));
            let now = tokio::time::Instant::now();
            let game = tokio::time::sleep_until(self.game_deadline.unwrap_or(now + IDLE_TIMEOUT));
            // Bot seats (ADR-0014): if any bot has a move to make, play it
            // after a short think delay, anchored to the last progress so
            // moves stay evenly paced.
            let bot_action = self.next_bot_action();
            let bot_armed = bot_action.is_some();
            let bot = tokio::time::sleep_until(last_progress + BOT_THINK);
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    let now = tokio::time::Instant::now();
                    last_activity = now;
                    // Snapshot who was acting and for how long BEFORE
                    // dispatch, since applying a game command can advance
                    // whose turn it is. Drain speculatively so a Game
                    // command's own Update (built inside handle_game,
                    // below) already reflects the spend; refunded after if
                    // the command turns out rejected (which sends
                    // Rejected, not Update, so the brief mutation is never
                    // observed) - ADR-0023.
                    let drain_seat = self.acting_seat();
                    let elapsed = now.saturating_duration_since(last_progress);
                    let drained = self.drain_bank(drain_seat, elapsed);
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
                        RoomCmd::PlayAgain { player_id } => self.handle_play_again(&player_id),
                        RoomCmd::AddBot { player_id } => {
                            self.handle_add_bot(&player_id);
                            false
                        }
                        RoomCmd::RemoveBot { player_id } => {
                            self.handle_remove_bot(&player_id);
                            false
                        }
                        RoomCmd::Configure {
                            player_id,
                            settings,
                        } => {
                            self.handle_configure(&player_id, settings);
                            false
                        }
                        RoomCmd::Game { player_id, cmd } => self.handle_game(&player_id, cmd),
                        RoomCmd::Feedback { player_id, rating, comment } => {
                            self.handle_feedback(&player_id, rating, comment);
                            false
                        }
                    };
                    // Any accepted command resets the turn clock and keeps
                    // the drain above; a rejected one gets it refunded.
                    if advanced {
                        last_progress = tokio::time::Instant::now();
                    } else {
                        self.refund_bank(drain_seat, drained);
                    }
                }
                _ = afk, if afk_armed => {
                    // The sleep target already covers turn_seconds plus the
                    // whole remaining bank, so firing means it's fully
                    // spent (ADR-0023) - zero it, but only for a connected
                    // seat: a disconnected seat is skipped by
                    // DISCONNECTED_GRACE alone and keeps its bank.
                    if let Some(seat) = self.acting_seat()
                        && self.seats.get(seat).is_some_and(|s| s.tx.is_some())
                        && let Some(remaining) = self.banks.get_mut(seat)
                    {
                        *remaining = 0;
                    }
                    if let Some((player, kind)) = self.afk_command() {
                        info!(room = %self.code, player = %player, action = ?kind,
                              "turn timeout, playing canonical action");
                        self.handle_game(&player, kind);
                    }
                    last_progress = tokio::time::Instant::now();
                }
                _ = game, if game_armed => {
                    self.handle_game_timeout();
                }
                _ = bot, if bot_armed => {
                    // Safety net: if a bot's smart move keeps getting
                    // rejected the afk timer still auto-plays a canonical
                    // action, so this never spins the game.
                    if let Some((player, kind)) = bot_action {
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
    /// is skipped after `DISCONNECTED_GRACE` whether or not a turn limit is
    /// set - the personal time bank does not apply to them (ADR-0023,
    /// pulling the plug earns no extra time). A connected but slow player
    /// gets the room's turn limit extended by whatever bank they have left.
    fn afk_deadline(&self) -> Option<Duration> {
        let seat = self.acting_seat()?;
        let turn_limit = self.settings.turn_seconds.map(Duration::from_secs);
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
    fn drain_bank(&mut self, seat: Option<usize>, elapsed: Duration) -> u64 {
        let Some(seat) = seat else { return 0 };
        if self.seats.get(seat).is_none_or(|s| s.tx.is_none()) {
            return 0;
        }
        let Some(turn_limit) = self.settings.turn_seconds.map(Duration::from_secs) else {
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
    fn refund_bank(&mut self, seat: Option<usize>, amount: u64) {
        if amount == 0 {
            return;
        }
        if let Some(seat) = seat
            && let Some(remaining) = self.banks.get_mut(seat)
        {
            *remaining += amount;
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

    /// The first bot seat with something to do right now and the command it
    /// wants, using the shared engine heuristic over that seat's own view
    /// (ADR-0014). `None` when no bot is waiting - covers turns, auctions,
    /// and declining trades offered to a bot.
    fn next_bot_action(&self) -> Option<(PlayerId, CommandKind)> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        for (i, seat) in self.seats.iter().enumerate() {
            if !seat.is_bot {
                continue;
            }
            let view = ClientView::for_seat(st, i);
            // The engine's content carries the effective rules after
            // start_game rebuilds it (ADR-0015), so the bot plays by the
            // room's actual settings.
            if let Some(kind) = parcello_engine::bot::decide(self.engine.content(), &view, i) {
                return Some((st.players[i].id.clone(), kind));
            }
        }
        None
    }

    fn is_host(&self, player_id: &str) -> bool {
        self.seats
            .first()
            .is_some_and(|s| s.identity.player_id == player_id)
    }

    /// Host adds a bot seat in the lobby (ADR-0014). Bots fill up to
    /// `MAX_PLAYERS` but are evicted again when a human joins a full room.
    fn handle_add_bot(&mut self, player_id: &str) {
        if !matches!(self.phase, Phase::Lobby) {
            return self.send_error(player_id, "bots can only be added in the lobby");
        }
        if !self.is_host(player_id) {
            return self.send_error(player_id, "only the host can add bots");
        }
        if self.seats.len() >= MAX_PLAYERS {
            return self.send_error(player_id, "room is full");
        }
        self.bot_counter += 1;
        let n = self.bot_counter;
        self.seats.push(Seat {
            identity: Identity {
                player_id: format!("bot:{n}"),
                name: format!("Bot {n}"),
                spoofable: false,
            },
            tx: None,
            reconnect: String::new(),
            feedback_given: false,
            is_bot: true,
        });
        info!(room = %self.code, bot = n, "bot added");
        self.broadcast_lobby();
    }

    /// Host drops the most recently added bot seat (lobby only).
    fn handle_remove_bot(&mut self, player_id: &str) {
        if !matches!(self.phase, Phase::Lobby) {
            return self.send_error(player_id, "bots can only be removed in the lobby");
        }
        if !self.is_host(player_id) {
            return self.send_error(player_id, "only the host can remove bots");
        }
        if let Some(i) = self.seats.iter().rposition(|s| s.is_bot) {
            self.seats.remove(i);
            self.broadcast_lobby();
        }
    }

    /// Host replaces the room's settings from the lobby (ADR-0015). The wire
    /// values are untrusted, so every field is clamped to a sane range before
    /// it is applied; the clamped result is broadcast back so all clients
    /// (and the host's own inputs) converge on what the server accepted.
    fn handle_configure(&mut self, player_id: &str, settings: RoomSettings) {
        if !matches!(self.phase, Phase::Lobby) {
            return self.send_error(player_id, "settings can only change in the lobby");
        }
        if !self.is_host(player_id) {
            return self.send_error(player_id, "only the host can change settings");
        }
        self.settings = clamp_settings(settings);
        info!(room = %self.code, "settings updated");
        self.broadcast_lobby();
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
                    // Bots yield to humans (ADR-0014): drop one to seat the
                    // newcomer; only genuinely full-of-humans rooms reject.
                    match self.seats.iter().rposition(|s| s.is_bot) {
                        Some(bot_i) => {
                            self.seats.remove(bot_i);
                        }
                        None => return Err("room is full".into()),
                    }
                }
                self.seats.push(Seat {
                    identity,
                    tx: Some(tx.clone()),
                    reconnect: new_reconnect_token(),
                    feedback_given: false,
                    is_bot: false,
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
            content: Box::new(self.effective_resolved()),
            view,
            reconnect: Some(self.seats[seat_index].reconnect.clone()),
            time_remaining: self.time_remaining_secs(),
            turn_seconds: self.settings.turn_seconds,
            time_bank_seconds: self.configured_time_bank(),
            settings: self.settings.clone(),
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
        if !self.is_host(player_id) {
            self.send_error(player_id, "only the host can start the game");
            return false;
        }
        if self.seats.len() < MIN_PLAYERS {
            self.send_error(player_id, "need at least 2 players");
            return false;
        }
        self.start_game(player_id)
    }

    /// Replay in the same room after a game ends: the first requester restarts
    /// for everyone still connected; seats that left are dropped. Returns true
    /// when a new game actually started (turn clock should reset).
    fn handle_play_again(&mut self, player_id: &str) -> bool {
        if !matches!(self.phase, Phase::Finished(_)) {
            self.send_error(player_id, "no finished game to replay");
            return false;
        }
        let connected = self.seats.iter().filter(|s| s.tx.is_some()).count();
        if connected < MIN_PLAYERS {
            self.send_error(player_id, "need at least 2 connected players to replay");
            return false;
        }
        // Drop players who returned to the start screen; re-seat the rest.
        self.seats.retain(|s| s.tx.is_some());
        self.start_game(player_id)
    }

    /// Base content with the host's effective rules applied (ADR-0015).
    fn effective_resolved(&self) -> ResolvedContent {
        let mut resolved = (*self.content).clone();
        resolved.content.rules = self.settings.rules.clone();
        resolved
    }

    /// Deals a fresh game to the current seats and broadcasts it. Shared by
    /// the first Start and every PlayAgain. Returns true when the game
    /// actually started (turn clock should reset). Rebuilds the engine with
    /// the host's chosen rules (ADR-0015), which is why it can fail.
    fn start_game(&mut self, host: &str) -> bool {
        let engine = match Engine::new(Arc::new(self.effective_resolved().content)) {
            Ok(engine) => engine,
            Err(e) => {
                self.send_error(host, &format!("invalid settings: {e}"));
                return false;
            }
        };
        self.engine = engine;
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
        // Start the game clock now, if the room is time-boxed (ADR-0010), and
        // reopen the post-game survey for the new game.
        self.game_deadline = self
            .settings
            .game_seconds
            .map(|s| tokio::time::Instant::now() + Duration::from_secs(s));
        for seat in &mut self.seats {
            seat.feedback_given = false;
        }
        // Rebuilt from scratch, not extended: seat renumbering across
        // PlayAgain must never carry a stale bank (ADR-0023).
        self.banks = vec![self.settings.time_bank_seconds.unwrap_or(0); self.seats.len()];
        info!(room = %self.code, players = self.seats.len(), "game started");

        let remaining = self.time_remaining_secs();
        let turn_seconds = self.settings.turn_seconds;
        let time_bank_seconds = self.configured_time_bank();
        let msgs: Vec<ServerMessage> = (0..self.seats.len())
            .map(|seat| ServerMessage::GameStarted {
                view: Box::new(ClientView::for_seat(&state, seat)),
                time_remaining: remaining,
                turn_seconds,
                time_bank_seconds,
            })
            .collect();
        self.phase = Phase::Active(state);
        self.send_per_seat(msgs);
        true
    }

    /// Seconds left before the game clock ends the game, if time-boxed.
    fn time_remaining_secs(&self) -> Option<u64> {
        self.game_deadline.map(|d| {
            d.saturating_duration_since(tokio::time::Instant::now())
                .as_secs()
        })
    }

    /// The configured time bank, normalized so `Some(0)` (host explicitly
    /// set it to zero via `Configure`) and `None` both read as "disabled"
    /// everywhere this rides the wire (ADR-0023).
    fn configured_time_bank(&self) -> Option<u64> {
        self.settings.time_bank_seconds.filter(|&s| s > 0)
    }

    /// Live per-seat remaining bank for `Update.banks`; `None` when the
    /// room has no time bank configured.
    fn banks_field(&self) -> Option<Vec<u64>> {
        self.configured_time_bank().map(|_| self.banks.clone())
    }

    /// Game clock expired: the richest player wins by net worth (ADR-0010).
    fn handle_game_timeout(&mut self) {
        let Phase::Active(state) = &self.phase else {
            return;
        };
        let (next, events) = self.engine.finish_on_time(state);
        let GamePhase::Finished { winner } = next.phase else {
            return;
        };
        let banks = self.banks_field();
        let msgs: Vec<ServerMessage> = (0..self.seats.len())
            .map(|seat| ServerMessage::Update {
                events: events.clone(), // TimeUp is public
                view: Box::new(ClientView::for_seat(&next, seat)),
                banks: banks.clone(),
            })
            .collect();
        self.phase = Phase::Finished(next);
        self.game_deadline = None;
        self.send_per_seat(msgs);
        let winner_id = self
            .seats
            .get(winner)
            .map(|s| s.identity.player_id.as_str());
        self.history.record_end(&self.code, winner_id);
        info!(room = %self.code, winner = ?winner_id, "game finished on time (richest wins)");
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
                let banks = self.banks_field();
                let msgs: Vec<ServerMessage> = (0..self.seats.len())
                    .map(|seat| ServerMessage::Update {
                        events: events
                            .iter()
                            .filter(|e| event_visible_to(e, seat))
                            .cloned()
                            .collect(),
                        view: Box::new(ClientView::for_seat(&next, seat)),
                        banks: banks.clone(),
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
                is_bot: s.is_bot,
            })
            .collect()
    }

    fn broadcast_lobby(&mut self) {
        let players = self.seat_infos();
        let settings = self.settings.clone();
        self.broadcast(ServerMessage::Lobby { players, settings });
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

    #[test]
    fn room_codes_are_pronounceable() {
        const CONSONANTS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
        const VOWELS: &[u8] = b"AEIOUY";
        for _ in 0..300 {
            let code = random_code();
            let b = code.as_bytes();
            assert_eq!(b.len(), 5, "{code}");
            for i in [0, 2, 4] {
                assert!(
                    CONSONANTS.contains(&b[i]),
                    "pos {i} not a consonant: {code}"
                );
            }
            for i in [1, 3] {
                assert!(VOWELS.contains(&b[i]), "pos {i} not a vowel: {code}");
            }
        }
    }

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
        let code = create_room(&rooms, content, history, None, None, None)
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

    /// After a game ends, `PlayAgain` restarts it in the same room for the
    /// players still connected.
    #[tokio::test]
    async fn play_again_restarts_the_game() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(&rooms, content, history, None, None, None)
            .await
            .expect("room created");
        let room = rooms.read().await.get(&code).cloned().expect("room handle");

        let mut rxs = Vec::new();
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
            rxs.push(rx);
        }
        room.send(RoomCmd::Start {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");
        // Bob resigns -> alice wins -> the game is finished.
        room.send(RoomCmd::Game {
            player_id: "guest:bob".into(),
            cmd: CommandKind::Resign,
        })
        .await
        .expect("room task alive");
        tokio::time::sleep(Duration::from_millis(150)).await;
        for rx in &mut rxs {
            while rx.try_recv().is_ok() {} // drain up to the finish
        }

        room.send(RoomCmd::PlayAgain {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");
        tokio::time::sleep(Duration::from_millis(150)).await;
        for (i, rx) in rxs.iter_mut().enumerate() {
            let mut restarted = false;
            while let Ok(msg) = rx.try_recv() {
                if matches!(msg, ServerMessage::GameStarted { .. }) {
                    restarted = true;
                }
            }
            assert!(restarted, "seat {i} must be pulled into the new game");
        }

        // The new game is live: a roll from the acting player is accepted.
        room.send(RoomCmd::Game {
            player_id: "guest:alice".into(),
            cmd: CommandKind::Roll,
        })
        .await
        .expect("room task alive");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut rolled = false;
        while let Ok(msg) = rxs[1].try_recv() {
            if let ServerMessage::Update { events, .. } = msg
                && events.iter().any(|e| matches!(e, Event::DiceRolled { .. }))
            {
                rolled = true;
            }
        }
        assert!(rolled, "the replayed game must accept commands");
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
        let code = create_room(&rooms, content, history, None, None, None)
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
        let code = create_room(&rooms, content, history, None, None, None)
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
                    Ok(ServerMessage::Update { events, view, .. }) => break (events, view),
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
        let code = create_room(&rooms, content, history, None, None, None)
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

    /// A time-boxed game ends on its own when the clock expires: both
    /// players start equal (net worth tie), so seat 0 wins, and the game
    /// becomes Finished (ADR-0010).
    #[tokio::test(start_paused = true)]
    async fn game_clock_finishes_by_net_worth() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(
            &rooms,
            content,
            history,
            None,
            None,
            Some(Duration::from_secs(60)),
        )
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
        // Confirm the countdown reaches the clients on GameStarted.
        room.send(RoomCmd::Start {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");

        let outcome = tokio::time::timeout(Duration::from_secs(600), async {
            let mut saw_countdown = false;
            while let Some(msg) = client_rxs[1].recv().await {
                match msg {
                    ServerMessage::GameStarted { time_remaining, .. } => {
                        saw_countdown = time_remaining == Some(60);
                    }
                    ServerMessage::Update { events, .. } => {
                        if let Some(Event::TimeUp { winner }) =
                            events.iter().find(|e| matches!(e, Event::TimeUp { .. }))
                        {
                            return (saw_countdown, *winner);
                        }
                    }
                    _ => {}
                }
            }
            (saw_countdown, usize::MAX)
        })
        .await
        .expect("the game clock must fire before the mock-clock timeout");
        assert!(outcome.0, "GameStarted must carry the 60s countdown");
        assert_eq!(
            outcome.1, 0,
            "an equal-net-worth tie awards the lowest seat"
        );

        // The game is now finished: further play is rejected.
        room.send(RoomCmd::Game {
            player_id: "guest:alice".into(),
            cmd: CommandKind::Roll,
        })
        .await
        .expect("room task alive");
        let rejected = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(msg) = client_rxs[0].recv().await {
                if matches!(msg, ServerMessage::Rejected { .. }) {
                    return true;
                }
            }
            false
        })
        .await
        .expect("a rejection must arrive");
        assert!(rejected, "the game must be finished after the time limit");
    }

    /// `--turn-timeout` is surfaced to clients as `turn_seconds` on
    /// GameStarted so they can show a per-turn countdown; off by default.
    #[tokio::test(start_paused = true)]
    async fn game_started_carries_turn_seconds() {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(
            &rooms,
            content,
            history,
            Some(Duration::from_secs(30)),
            Some(Duration::from_secs(45)),
            None,
        )
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

        let started = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(msg) = client_rxs[1].recv().await {
                if let ServerMessage::GameStarted {
                    turn_seconds,
                    time_remaining,
                    time_bank_seconds,
                    ..
                } = msg
                {
                    return (turn_seconds, time_remaining, time_bank_seconds);
                }
            }
            (None, None, None)
        })
        .await
        .expect("GameStarted must arrive");
        assert_eq!(started.0, Some(30), "turn timer must reach the client");
        assert_eq!(started.1, None, "untimed game carries no game clock");
        assert_eq!(started.2, Some(45), "time bank must reach the client");
    }

    fn base_content() -> Arc<ResolvedContent> {
        Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        )
    }

    fn test_room(content: Arc<ResolvedContent>) -> Room {
        let engine = Engine::new(Arc::new(content.content.clone())).expect("engine builds");
        let settings = RoomSettings {
            game_seconds: None,
            turn_seconds: None,
            time_bank_seconds: None,
            rules: content.content.rules.clone(),
        };
        Room {
            code: "TESTS".into(),
            content,
            engine,
            seats: Vec::new(),
            phase: Phase::Lobby,
            history: Arc::new(MemoryHistory::new()),
            settings,
            game_deadline: None,
            bot_counter: 0,
            banks: Vec::new(),
        }
    }

    fn human_seat(id: &str) -> Seat {
        Seat {
            identity: Identity {
                player_id: id.into(),
                name: id.into(),
                spoofable: true,
            },
            tx: None,
            reconnect: String::new(),
            feedback_given: false,
            is_bot: false,
        }
    }

    /// A bot seat produces the engine heuristic's move for its own turn: a
    /// fresh game opens on seat 0 with a roll (ADR-0014).
    #[test]
    fn bot_seat_acts_on_its_turn() {
        let content = base_content();
        let engine = Engine::new(Arc::new(content.content.clone())).unwrap();
        let state = engine.new_game(
            vec![
                ("bot:1".into(), "Bot 1".into()),
                ("guest:al".into(), "Al".into()),
            ],
            7,
        );
        let mut room = test_room(content);
        room.seats = vec![human_seat("bot:1"), human_seat("guest:al")];
        room.seats[0].is_bot = true;
        room.phase = Phase::Active(state);
        assert!(matches!(
            room.next_bot_action(),
            Some((id, CommandKind::Roll)) if id == "bot:1"
        ));
    }

    /// Bots fill a room but never lock a human out: joining a full room
    /// evicts a bot to seat the newcomer (ADR-0014).
    #[test]
    fn bots_yield_their_seat_to_a_joining_human() {
        let mut room = test_room(base_content());
        room.seats.push(human_seat("guest:host"));
        while room.seats.len() < MAX_PLAYERS {
            room.handle_add_bot("guest:host");
        }
        assert_eq!(room.seats.len(), MAX_PLAYERS);
        let bots_before = room.seats.iter().filter(|s| s.is_bot).count();

        let (tx, _rx) = mpsc::unbounded_channel();
        room.handle_join(
            Identity {
                player_id: "guest:new".into(),
                name: "New".into(),
                spoofable: true,
            },
            None,
            tx,
        )
        .expect("a human must displace a bot in a full room");

        assert_eq!(room.seats.len(), MAX_PLAYERS, "room stays at capacity");
        assert!(
            room.seats
                .iter()
                .any(|s| s.identity.player_id == "guest:new"),
            "the human took a seat"
        );
        assert_eq!(
            room.seats.iter().filter(|s| s.is_bot).count(),
            bots_before - 1,
            "exactly one bot yielded"
        );
        assert!(!room.seats[0].is_bot, "the host keeps seat 0");
    }

    /// Only the host may add bots, and only in the lobby.
    #[test]
    fn add_bot_is_host_and_lobby_gated() {
        let mut room = test_room(base_content());
        room.seats.push(human_seat("guest:host"));
        room.seats.push(human_seat("guest:other"));
        room.handle_add_bot("guest:other"); // not the host
        assert!(room.seats.iter().all(|s| !s.is_bot), "non-host cannot add");
        room.handle_add_bot("guest:host");
        assert_eq!(room.seats.iter().filter(|s| s.is_bot).count(), 1);
        room.handle_remove_bot("guest:host");
        assert!(room.seats.iter().all(|s| !s.is_bot), "host removed the bot");
    }

    /// Only the host may change settings, and the server clamps absurd wire
    /// values (ADR-0015).
    #[test]
    fn configure_is_host_gated_and_clamped() {
        let mut room = test_room(base_content());
        room.seats.push(human_seat("guest:host"));
        room.seats.push(human_seat("guest:other"));
        let before = room.settings.rules.starting_balance;

        let mut s = room.settings.clone();
        s.rules.starting_balance = 5000;
        room.handle_configure("guest:other", s); // not the host
        assert_eq!(
            room.settings.rules.starting_balance, before,
            "a non-host must not change settings"
        );

        let mut s = room.settings.clone();
        s.rules.starting_balance = i64::MAX;
        s.rules.max_houses_per_property = 200;
        s.game_seconds = Some(1); // below the 60s floor
        s.turn_seconds = Some(25);
        s.time_bank_seconds = Some(10_000); // above the 600s ceiling
        room.handle_configure("guest:host", s);
        assert_eq!(room.settings.rules.starting_balance, 1_000_000, "clamped");
        assert_eq!(room.settings.rules.max_houses_per_property, 5, "house cap");
        assert_eq!(room.settings.game_seconds, Some(60), "game floor");
        assert_eq!(room.settings.turn_seconds, Some(25));
        assert_eq!(room.settings.time_bank_seconds, Some(600), "bank ceiling");
    }

    /// The game starts with the host's chosen rules: a rebuilt engine deals
    /// the configured starting balance (ADR-0015).
    #[test]
    fn start_game_applies_the_host_rules() {
        let mut room = test_room(base_content());
        room.seats.push(human_seat("guest:host"));
        room.seats.push(human_seat("guest:bob"));
        room.settings.rules.starting_balance = 777;
        assert!(room.start_game("guest:host"), "game must start");
        let Phase::Active(st) = &room.phase else {
            panic!("game should be active");
        };
        assert!(
            st.players.iter().all(|p| p.cash == 777),
            "every player starts with the host's balance"
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
        let code = create_room(
            &rooms,
            content,
            history,
            Some(Duration::from_secs(30)),
            None, // no time bank: a plain hard stop at the turn limit
            None,
        )
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

    /// Setup shared by the time-bank tests below: a two-player room with the
    /// given turn limit and bank, started and ready for seat 0 (alice) to
    /// act.
    async fn started_room_with_bank(
        turn_timeout: Option<Duration>,
        time_bank: Option<Duration>,
    ) -> (
        mpsc::Sender<RoomCmd>,
        Vec<mpsc::UnboundedReceiver<ServerMessage>>,
    ) {
        let content = Arc::new(
            parcello_mods::resolve(
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
                &["base".to_string()],
            )
            .expect("base mod loads"),
        );
        let rooms = Rooms::default();
        let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
        let code = create_room(&rooms, content, history, turn_timeout, time_bank, None)
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
        (room, client_rxs)
    }

    /// A connected seat that overruns the plain turn window but acts before
    /// its bank is dry is NOT auto-played; the overage is permanently
    /// deducted from its bank instead (ADR-0023).
    #[tokio::test(start_paused = true)]
    async fn time_bank_absorbs_overrun_without_auto_play() {
        let (room, mut client_rxs) =
            started_room_with_bank(Some(Duration::from_secs(5)), Some(Duration::from_secs(20)))
                .await;

        // Alice stalls 7s (5s over the turn limit, well inside the 20s
        // bank) then rolls herself - the bank absorbs the 2s overage.
        tokio::time::sleep(Duration::from_secs(7)).await;
        room.send(RoomCmd::Game {
            player_id: "guest:alice".into(),
            cmd: CommandKind::Roll,
        })
        .await
        .expect("room task alive");

        let (rolled_herself, banks) = tokio::time::timeout(Duration::from_secs(60), async {
            while let Some(msg) = client_rxs[1].recv().await {
                if let ServerMessage::Update { events, banks, .. } = msg {
                    let rolled = events.iter().any(|e| matches!(e, Event::DiceRolled { .. }));
                    if rolled {
                        return (true, banks);
                    }
                }
            }
            (false, None)
        })
        .await
        .expect("an update must arrive");
        assert!(rolled_herself, "alice's own roll must be accepted");
        assert_eq!(
            banks,
            Some(vec![18, 20]),
            "alice's bank drains by the 2s overage; bob's is untouched"
        );
    }

    /// Once the plain turn window AND the whole bank are exhausted, the
    /// canonical action auto-plays and the bank reads zero (ADR-0023).
    #[tokio::test(start_paused = true)]
    async fn time_bank_hard_stops_when_exhausted() {
        let (_room, mut client_rxs) =
            started_room_with_bank(Some(Duration::from_secs(5)), Some(Duration::from_secs(3)))
                .await;

        let (auto_rolled, banks) = tokio::time::timeout(Duration::from_secs(300), async {
            while let Some(msg) = client_rxs[1].recv().await {
                if let ServerMessage::Update { events, banks, .. } = msg
                    && events.iter().any(|e| matches!(e, Event::DiceRolled { .. }))
                {
                    return (true, banks);
                }
            }
            (false, None)
        })
        .await
        .expect("an update must arrive before the mock-clock timeout");
        assert!(auto_rolled, "AFK timer should roll once the bank is dry");
        assert_eq!(banks, Some(vec![0, 3]), "alice's bank is fully spent");
    }

    /// A disconnected seat is skipped after `DISCONNECTED_GRACE` alone; the
    /// time bank never extends that (ADR-0023: pulling the plug earns no
    /// extra time).
    #[tokio::test(start_paused = true)]
    async fn disconnected_seat_ignores_the_time_bank() {
        let (room, mut client_rxs) = started_room_with_bank(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(1000)),
        )
        .await;
        room.send(RoomCmd::Disconnect {
            player_id: "guest:alice".into(),
        })
        .await
        .expect("room task alive");

        let auto_rolled = tokio::time::timeout(Duration::from_secs(120), async {
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
        .expect("must roll within DISCONNECTED_GRACE, far short of the 1000s bank");
        assert!(auto_rolled, "a disconnected seat must not draw on its bank");
    }
}
