# ADR-0035: spectators, and a bots showcase game as a last resort

Status: accepted

## Context

Players on the public server asked to watch a running game without playing
in it - both to follow friends and, mostly, to learn how a game flows
before joining one (the same onboarding gap the coach-mark hints address
client-side). `docs/architecture.typ` describes a strict player/seat
model: every connection in a room holds a seat, every view is
`ClientView::for_seat`. Watching without a seat is therefore a deviation
that needs this ADR. The owner also wants something to watch even on an
empty server: a bots-only game running as a *last resort*, only when no
humans are playing.

## Decision

**Spectator view.** A new pure constructor,
`ClientView::for_spectator(state, content)`: the omniscient `of` minus
everything seat-private. Concretely - no pending trade offers at all
(ADR-0007 makes them private to their two parties; a spectator is party to
none), and every pending sealed bid and bribe vote masked while its window
is open (ADR-0018/0024 secrecy: a seat sees only *its own* pending entry,
and a spectator has none). Resolution events stay public as always. The
never-expose list (`rng`, deck order) is untouched - `of` already excludes
them.

**Spectator connections.** New wire messages, additive as usual:

- `spectate {code?, auth}` - authenticate exactly like `join` (same
  verifier, same trust model; on a token-only server you sign in to watch
  too). With a `code`, watch that room; without one, the server probes its
  rooms and picks the most-watched-worthy: an Active game with the most
  connected humans, else the bots showcase, else an explicit error.
- `spectating {code, players, content, view?, settings, ...}` - the
  spectator's mirror of `joined`: same room context, no `seat`, no
  `reconnect` (nothing to protect - a spectator holds nothing).

Spectators ride the room's existing broadcasts: `lobby` updates (public
by definition), `game_started`/`update` carrying the spectator view with
trade lifecycle events filtered out. They are NOT seats: their
`animation_done` acks are ignored (they can never gate the table's timers,
ADR-0028 - a slow viewer lags behind, the game does not wait), they hold
no reconnect token, and every game command from a spectator session is
refused at the transport (`ws.rs` marks the session; only leaving is
allowed). `MAX_SPECTATORS` = 32 per room bounds the fan-out. A room
dissolves on idle only when it has no connected seats AND no spectators -
a watched game stays alive.

**Bots showcase (`--showcase`, off by default).** A supervisor task (same
shape as the ranked matchmaker, ADR-0034) ticks every 15s: if no room has
an Active game with at least one connected human, and no showcase room
exists, it creates one - four bot seats (ADR-0014), server-default
settings, auto-started at creation. The room replays itself (the shared
`start_game`, ~10s after each finish) for as long as it lives. It is
deliberately NOT killed the moment humans start playing: the supervisor
just stops recreating it, and the room winds down through the normal idle
timeout once nobody is watching. Nobody can join it as a player (it is
never in a joinable lobby phase).

## Consequences

- The engine gains one pure view constructor; rules, replay format, and
  the existing `for_seat` call sites are untouched.
- The wire grows two additive messages; `update` reuses its shape
  verbatim (a spectator client renders exactly what a player client
  renders, minus its own action panel).
- The clients gain a "watch a game" path that doubles as the tutorial's
  second half: watching bots play IS the demo. The Flutter client renders
  spectating as a game screen with no action panel; the CLI gets
  `--spectate [CODE]` as the cheapest end-to-end check.
- An unwatched showcase burns one room of bot turns (~1 command per
  800ms) for up to the 30-minute idle timeout; acceptable, and opt-in via
  the flag.
- Deferred: spectator chat, a public game list (`spectate` without a code
  is the discovery mechanism for now), spectating ranked games
  differently (they are watchable like any room).
