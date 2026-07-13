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
pub const IDLE_TIMEOUT: Duration = Duration::from_mins(30);
/// A disconnected player's turn is auto-played after this grace even when
/// `--turn-timeout` is off: someone who left must never stall the table.
///
/// The grace lets a brief network blip recover (they keep their seat and
/// its reconnect token, ADR-0008).
// ponytail: fixed 30s; make it a flag only if operators ask to tune it.
pub const DISCONNECTED_GRACE: Duration = Duration::from_secs(30);
/// A bot seat pauses this long before each move so humans can follow the
/// action; without it a table of bots would resolve instantly (ADR-0014).
const BOT_THINK: Duration = Duration::from_millis(800);
/// Floor on the plain-turn window for a jailed seat choosing its exit
/// (Legal Route / Corruption / jail card, ADR-0024): a genuine multi-way
/// decision needs more room than an ordinary blitz turn, so the effective
/// limit is `max(settings.turn_seconds, this)` rather than the room's
/// plain default (2026-07 playtest feedback).
const JAIL_DECISION_SECS: u64 = 20;
/// Sealed-bid auction window (ADR-0018): every living seat has this long to
/// submit a bid once a window opens; silent seats are auto-abstained at
/// expiry. A separate, parallel timer from the turn clock/time bank -
/// `acting_seat()` returns `None` for the whole duration, so neither is
/// consumed while it's armed. 12s (not the original 5s): with real
/// players, quick-bid buttons still need enough headroom to read the
/// property and react (2026-07 playtest feedback).
const BID_WINDOW: Duration = Duration::from_secs(12);
/// Corruption bribe vote window (ADR-0024): the same timed-collection-window
/// pattern as `BID_WINDOW`, kept as its own independently-armed timer rather
/// than a shared primitive - matching how `game_deadline` and `bid_deadline`
/// already coexist as two small parallel `Option<Instant>` fields.
const VOTE_WINDOW: Duration = Duration::from_secs(5);
/// Hard cap on waiting for animation acks (ADR-0028): a client that never
/// acks (bug, malice, a throttled background tab) can delay an
/// animation-gated timer by at most this much, ever - the same
/// wait-but-never-indefinitely doctrine as `DISCONNECTED_GRACE` and the
/// window auto-abstains. The absolute `game_deadline` (ADR-0010) is
/// deliberately NOT gated at all.
///
/// This is the ceiling the client's own `ANIM_BUDGET` sits under (ADR-0030,
/// tiered 8s/6s/4s by the loudest beat in the Update, plus a 2s margin for
/// frame-rate slop). The two constants are coupled BY CONTRACT: raising the
/// client budget without raising this reopens the exact desync ADR-0028
/// exists to prevent - the server un-gates and the client is left animating a
/// turn the table has already moved past.
const ANIM_ACK_CAP: Duration = Duration::from_secs(10);

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
    /// This client finished rendering every Update through `through_seq`
    /// (ADR-0028); releases the animation gates on the room's timers.
    AnimationDone {
        player_id: PlayerId,
        through_seq: u64,
    },
}

/// Creates the room task and registers its handle. Returns the room code.
///
/// # Errors
/// When the resolved content fails engine validation (a room must never
/// start on rules the engine would reject).
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
        bid_deadline: None,
        vote_deadline: None,
        seq: 0,
        acked: Vec::new(),
        anim_broadcast_at: tokio::time::Instant::now(),
        table_settled_at: None,
        acting_settled_at: None,
        bid_gate: false,
        vote_gate: false,
    };
    tokio::spawn(room.run(rx, Arc::clone(rooms)));
    Ok(code)
}

/// Trade lifecycle events are private to their two parties (ADR-0007);
/// everything else is public.
const fn event_visible_to(event: &Event, seat: usize) -> bool {
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

/// Bounds for host-supplied room settings (ADR-0015), named in one place
/// so the trust boundary is auditable at a glance. The wire values are
/// untrusted: absurd numbers would break the game (a house level past the
/// 6-entry rent table, negative economy) or the experience (a one-second
/// game).
mod limits {
    /// Game clock: one minute to one day.
    pub const GAME_SECONDS: (u64, u64) = (60, 86_400);
    /// Turn clock: 5s blitz floor to one hour.
    pub const TURN_SECONDS: (u64, u64) = (5, 3_600);
    /// Personal time bank (ADR-0023): up to ten minutes.
    pub const BANK_SECONDS: (u64, u64) = (0, 600);
    pub const STARTING_BALANCE: (i64, i64) = (0, 1_000_000);
    pub const GO_SALARY: (i64, i64) = (0, 100_000);
    /// Velocity deck (ADR-0017): `GameContent::validate` requires
    /// `velocity_min >= 1` and `velocity_max > velocity_min`; the u8 caps
    /// leave room for `min + 1` at the top.
    pub const VELOCITY_MIN: (u8, u8) = (1, 254);
    pub const VELOCITY_MAX_CEIL: u8 = 255;
    /// rents[] has six levels (0..=5, level 5 = hotel); a higher cap would
    /// index past the array in the rent calculator.
    pub const MAX_HOUSES: (u8, u8) = (1, 5);
    pub const BANKRUPTCY_THRESHOLD: (i64, i64) = (0, 1_000_000);
    /// Cost percents (ADR-0011/0012): up to 10x price.
    pub const COST_PCT: (i64, i64) = (0, 1_000);
    pub const WIN_FULL_GROUPS: (i64, i64) = (0, 100);
    pub const WIN_VICTORY_POINTS: (i64, i64) = (0, 500);
    /// Small multipliers (base mod uses 6/3), not percents - a much
    /// smaller ceiling is enough (ADR-0019).
    pub const POOL_FACTOR: (i64, i64) = (0, 100);
    /// A multiplier like the forecast's `magnitude_pct`, not a pure cost -
    /// the -100 floor keeps the spotlight rent calculator's `.max(0)`
    /// meaningful rather than degenerate (ADR-0026).
    pub const SPOTLIGHT_RENT_PCT: (i64, i64) = (-100, 1_000);
    pub const SPOTLIGHT_DURATION: (i64, i64) = (0, 500);
}

/// Clamp host-supplied room settings to the `limits` above (ADR-0015).
/// Returned settings are always safe to apply.
fn clamp_settings(mut s: RoomSettings) -> RoomSettings {
    fn to<T: Ord>(v: T, (lo, hi): (T, T)) -> T {
        v.clamp(lo, hi)
    }
    s.game_seconds = s.game_seconds.map(|v| to(v, limits::GAME_SECONDS));
    s.turn_seconds = s.turn_seconds.map(|v| to(v, limits::TURN_SECONDS));
    s.time_bank_seconds = s.time_bank_seconds.map(|v| to(v, limits::BANK_SECONDS));
    let r = &mut s.rules;
    r.starting_balance = to(r.starting_balance, limits::STARTING_BALANCE);
    r.go_salary = to(r.go_salary, limits::GO_SALARY);
    r.velocity_min = to(r.velocity_min, limits::VELOCITY_MIN);
    r.velocity_max = r
        .velocity_max
        .clamp(r.velocity_min + 1, limits::VELOCITY_MAX_CEIL);
    r.max_houses_per_property = to(r.max_houses_per_property, limits::MAX_HOUSES);
    r.bankruptcy_threshold = to(r.bankruptcy_threshold, limits::BANKRUPTCY_THRESHOLD);
    r.expropriation = to(r.expropriation, limits::COST_PCT);
    r.rent_boost = to(r.rent_boost, limits::COST_PCT);
    r.win_full_groups = to(r.win_full_groups, limits::WIN_FULL_GROUPS);
    r.win_victory_points = to(r.win_victory_points, limits::WIN_VICTORY_POINTS);
    r.subsidiary_pool_factor = to(r.subsidiary_pool_factor, limits::POOL_FACTOR);
    r.conglomerate_pool_factor = to(r.conglomerate_pool_factor, limits::POOL_FACTOR);
    r.spotlight_rent_pct = to(r.spotlight_rent_pct, limits::SPOTLIGHT_RENT_PCT);
    r.spotlight_duration_turns = to(r.spotlight_duration_turns, limits::SPOTLIGHT_DURATION);
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
    /// Deadline for the currently open sealed-bid window (ADR-0018), if
    /// any - armed in `handle_game` the instant `TurnPhase::BlindAuction`
    /// opens, cleared the instant it's no longer open. Independent of
    /// `last_progress`/`banks`: `acting_seat()` already returns `None` for
    /// the whole phase, so the normal turn machinery stays disarmed.
    bid_deadline: Option<tokio::time::Instant>,
    /// Deadline for the currently open Corruption bribe vote (ADR-0024), if
    /// any - armed and cleared exactly like `bid_deadline`, its structural
    /// twin for the other simultaneous multi-seat phase.
    vote_deadline: Option<tokio::time::Instant>,
    /// Monotonic Update counter (ADR-0028); every broadcast Update carries
    /// it so clients can ack "rendered through N".
    seq: u64,
    /// Highest Update seq each seat has acked, parallel to `seats`. Bot and
    /// disconnected seats are treated as instantly settled instead
    /// (`seat_settled`), so their entries just lag harmlessly.
    acked: Vec<u64>,
    /// Instant of the latest Update broadcast: the anchor for
    /// `ANIM_ACK_CAP`, past which every gate opens regardless of acks.
    anim_broadcast_at: tokio::time::Instant,
    /// When every relevant seat had rendered the latest Update (`None` =
    /// still waiting). Gates the sealed-bid/vote windows and bot pacing.
    /// Stamped by `refresh_gates`, reset on every broadcast.
    table_settled_at: Option<tokio::time::Instant>,
    /// Same, for the acting seat alone: gates the turn clock/time bank so
    /// animations never eat thinking time (ADR-0028).
    acting_settled_at: Option<tokio::time::Instant>,
    /// A sealed-bid window opened but its 5s deadline is not armed yet -
    /// waiting for `table_settled_at` (or the cap) so nobody's window
    /// starts before they have seen the landing (ADR-0028).
    bid_gate: bool,
    /// Same, for a Corruption bribe vote window.
    vote_gate: bool,
}

/// What a dispatched `RoomCmd` did to the room, as seen by the run loop's
/// clock bookkeeping.
enum Dispatched {
    /// The command advanced the game: reset the turn clock, keep the
    /// speculative time-bank drain.
    Advanced,
    /// Rejected or non-game command: refund the speculative drain.
    NoProgress,
    /// The last human left an empty lobby: dissolve the room.
    Dissolve,
}

impl Room {
    /// Routes one client command to its handler and reports what it did.
    fn dispatch(&mut self, cmd: RoomCmd) -> Dispatched {
        let advanced = match cmd {
            RoomCmd::Join {
                identity,
                reconnect,
                tx,
                reply,
            } => {
                let result = self.handle_join(identity, reconnect.as_deref(), &tx);
                let _ = reply.send(result);
                false
            }
            RoomCmd::Disconnect { player_id } => {
                if self.handle_disconnect(&player_id) {
                    return Dispatched::Dissolve;
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
            RoomCmd::Feedback {
                player_id,
                rating,
                comment,
            } => {
                self.handle_feedback(&player_id, rating, comment);
                false
            }
            RoomCmd::AnimationDone {
                player_id,
                through_seq,
            } => {
                self.handle_animation_done(&player_id, through_seq);
                false
            }
        };
        if advanced {
            Dispatched::Advanced
        } else {
            Dispatched::NoProgress
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<RoomCmd>, rooms: Rooms) {
        info!(room = %self.code, "room created");
        let mut last_activity = tokio::time::Instant::now();
        let mut last_progress = tokio::time::Instant::now();
        loop {
            // Animation gates first (ADR-0028): stamp the settle instants
            // and arm any pending bid/vote deadline before computing the
            // sleeps below from them.
            self.refresh_gates();
            let idle = tokio::time::sleep_until(last_activity + IDLE_TIMEOUT);
            // Smart per-turn deadline, recomputed each loop so a mid-turn
            // disconnect shortens it: disconnected acting seats are skipped
            // after a short grace (always on); a connected but slow player
            // gets the configured --turn-timeout, or unlimited time when off.
            // Anchored to the later of the last accepted command and the
            // acting seat's animation ack (ADR-0028): rendering time never
            // eats thinking time.
            let deadline = self.afk_deadline();
            let afk_armed = deadline.is_some();
            let afk_anchor = self.acting_anchor(last_progress);
            let afk = tokio::time::sleep_until(afk_anchor + deadline.unwrap_or(IDLE_TIMEOUT));
            // Game clock (ADR-0010): fires once at the absolute deadline.
            // Deliberately NOT animation-gated - a stalling client must
            // never be able to extend the game (ADR-0028).
            let game_armed = self.game_deadline.is_some() && matches!(self.phase, Phase::Active(_));
            let now = tokio::time::Instant::now();
            let game = tokio::time::sleep_until(self.game_deadline.unwrap_or(now + IDLE_TIMEOUT));
            // Bot seats (ADR-0014): if any bot has a move to make, play it
            // after a short think delay, anchored to the last progress AND
            // the table's animation watermark (ADR-0028) so bots never race
            // ahead of what the humans can see.
            let bot_action = self.next_bot_action();
            let bot_armed = bot_action.is_some();
            let bot = tokio::time::sleep_until(self.table_anchor(last_progress) + BOT_THINK);
            // Sealed-bid window (ADR-0018): a separate, parallel timer -
            // does not touch last_progress/banks, matching acting_seat()
            // returning None for the whole phase. Armed by refresh_gates
            // once the table has visually arrived (ADR-0028).
            let bid_armed = self.bid_deadline.is_some();
            let bid = tokio::time::sleep_until(self.bid_deadline.unwrap_or(now + IDLE_TIMEOUT));
            // Corruption bribe vote window (ADR-0024): the same pattern,
            // its own independent timer.
            let vote_armed = self.vote_deadline.is_some();
            let vote = tokio::time::sleep_until(self.vote_deadline.unwrap_or(now + IDLE_TIMEOUT));
            // Wake at the ack hard cap while a window waits on animations,
            // so refresh_gates arms its deadline even if nobody ever acks.
            // The afk/bot anchors need no wake: they fall back to the cap
            // instant directly in their own sleep targets.
            let gate_pending = (self.bid_gate || self.vote_gate) && self.table_settled_at.is_none();
            let gate = tokio::time::sleep_until(self.anim_broadcast_at + ANIM_ACK_CAP);
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
                    // Elapsed thinking time is measured from the animation
                    // anchor, not last_progress (ADR-0028): stalling while
                    // the table still renders must not drain the bank.
                    let elapsed = now.saturating_duration_since(afk_anchor);
                    let drained = self.drain_bank(drain_seat, elapsed);
                    match self.dispatch(cmd) {
                        // Last human left an empty lobby: the room is done.
                        Dispatched::Dissolve => break,
                        // Any accepted command resets the turn clock and
                        // keeps the drain above...
                        Dispatched::Advanced => {
                            last_progress = tokio::time::Instant::now();
                        }
                        // ...a rejected or non-game one gets it refunded.
                        Dispatched::NoProgress => self.refund_bank(drain_seat, drained),
                    }
                }
                () = afk, if afk_armed => {
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
                () = game, if game_armed => {
                    self.handle_game_timeout();
                }
                () = bot, if bot_armed => {
                    // Safety net: if a bot's smart move keeps getting
                    // rejected the afk timer still auto-plays a canonical
                    // action, so this never spins the game.
                    if let Some((player, kind)) = bot_action {
                        self.handle_game(&player, kind);
                    }
                    last_progress = tokio::time::Instant::now();
                }
                () = bid, if bid_armed => {
                    self.inject_silent_bids();
                }
                () = vote, if vote_armed => {
                    self.inject_silent_votes();
                }
                () = gate, if gate_pending => {
                    // Nothing to do here: reaching the cap is the signal;
                    // the refresh_gates call at the top of the next
                    // iteration stamps the settle instants and arms the
                    // pending window deadline (ADR-0028).
                }
                () = idle => {
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

    /// Seat expected to act right now, or `None` outside an active game -
    /// also `None` while a sealed-bid window or a Corruption bribe vote is
    /// open (ADR-0018/ADR-0024): every living seat may act there, not a
    /// single actor, so each is governed by its own parallel deadline timer
    /// instead (this is exactly what disarms the turn clock/time bank for
    /// the whole window's duration).
    const fn acting_seat(&self) -> Option<usize> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        match st.turn {
            TurnPhase::BlindAuction { .. } | TurnPhase::BribeVote { .. } => None,
            _ => Some(st.current),
        }
    }

    // -- Animation gates (ADR-0028) ---------------------------------------

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
        reconnect: Option<&str>,
        tx: &ClientTx,
    ) -> Result<(), String> {
        let seat_index = if let Some(i) = self.seat_of(&identity.player_id) {
            // Rejoin: last connection wins, but a spoofable (guest)
            // identity must prove seat ownership with the reconnect
            // token issued at first join (ADR-0008). JWT identities
            // are cryptographically bound and need no token.
            let proven = !self.seats[i].identity.spoofable
                || reconnect.is_some_and(|t| token_eq(t, &self.seats[i].reconnect));
            if !proven {
                return Err("seat is protected: rejoin with its reconnect token".into());
            }
            self.seats[i].tx = Some(tx.clone());
            info!(room = %self.code, player = %identity.player_id, seat = i, "rejoined");
            i
        } else {
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
        };

        let view =
            match &self.phase {
                Phase::Lobby => None,
                Phase::Active(state) | Phase::Finished(state) => Some(Box::new(
                    ClientView::for_seat(state, self.engine.content(), seat_index),
                )),
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
    /// the first Start and every `PlayAgain`. Returns true when the game
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
        // A fresh game always opens on AwaitMove, but clear defensively so
        // PlayAgain never carries a stale sealed-bid window (ADR-0018) or
        // bribe vote (ADR-0024).
        self.bid_deadline = None;
        self.vote_deadline = None;
        // Animation gates (ADR-0028): everyone is aligned at game start -
        // GameStarted is not an Update and needs no ack.
        self.acked = vec![self.seq; self.seats.len()];
        self.bid_gate = false;
        self.vote_gate = false;
        info!(room = %self.code, players = self.seats.len(), "game started");

        let remaining = self.time_remaining_secs();
        let turn_seconds = self.settings.turn_seconds;
        let time_bank_seconds = self.configured_time_bank();
        let msgs: Vec<ServerMessage> = (0..self.seats.len())
            .map(|seat| ServerMessage::GameStarted {
                view: Box::new(ClientView::for_seat(&state, self.engine.content(), seat)),
                time_remaining: remaining,
                turn_seconds,
                time_bank_seconds,
            })
            .collect();
        self.phase = Phase::Active(state);
        self.send_per_seat(msgs);
        true
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
        let seq = self.next_update_seq();
        let msgs: Vec<ServerMessage> = (0..self.seats.len())
            .map(|seat| ServerMessage::Update {
                seq,
                events: events.clone(), // TimeUp is public
                view: Box::new(ClientView::for_seat(&next, self.engine.content(), seat)),
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

        let was_blind_auction = matches!(state.turn, TurnPhase::BlindAuction { .. });
        let was_bribe_vote = matches!(state.turn, TurnPhase::BribeVote { .. });
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
                // Sealed-bid window timer (ADR-0018): a separate, parallel
                // clock from the turn timer/time bank. Opening no longer
                // arms it directly - it flags the gate, and refresh_gates
                // arms the 5s deadline once the table has visually arrived
                // at the tile (or the ack cap passes, ADR-0028). Cleared
                // the instant the window is no longer open - whether it
                // resolved or the game ended.
                let is_blind_auction = matches!(next.turn, TurnPhase::BlindAuction { .. });
                if is_blind_auction && !was_blind_auction {
                    self.bid_gate = true;
                    self.bid_deadline = None;
                } else if !is_blind_auction {
                    self.bid_gate = false;
                    self.bid_deadline = None;
                }
                // Corruption bribe vote window (ADR-0024): the same
                // transition-detection pattern, its own independent timer.
                let is_bribe_vote = matches!(next.turn, TurnPhase::BribeVote { .. });
                if is_bribe_vote && !was_bribe_vote {
                    self.vote_gate = true;
                    self.vote_deadline = None;
                } else if !is_bribe_vote {
                    self.vote_gate = false;
                    self.vote_deadline = None;
                }
                let finished_winner = match next.phase {
                    GamePhase::Finished { winner } => Some(winner),
                    GamePhase::Active => None,
                };
                // One view + event feed per seat: trade offers and their
                // lifecycle events reach only the two parties (ADR-0007).
                let banks = self.banks_field();
                let seq = self.next_update_seq();
                let msgs: Vec<ServerMessage> = (0..self.seats.len())
                    .map(|seat| ServerMessage::Update {
                        seq,
                        events: events
                            .iter()
                            .filter(|e| event_visible_to(e, seat))
                            .cloned()
                            .collect(),
                        view: Box::new(ClientView::for_seat(&next, self.engine.content(), seat)),
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
        self.broadcast(&ServerMessage::Lobby { players, settings });
    }

    fn broadcast(&mut self, msg: &ServerMessage) {
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

mod autoplay;
mod clock;

#[cfg(test)]
mod tests;
