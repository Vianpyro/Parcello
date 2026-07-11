# Parcello

Open-source multiplayer board game in the spirit of Business Tour: fast,
dynamic games rather than Monopoly's slow accumulation. Authoritative Rust
server, thin clients, community-hosted servers (Minecraft model),
data-driven mods. The current rules are still Monopoly-close; the target
design and the gap are tracked in `docs/business-tour-direction.md`.

This repository holds the complete, playable game: pure game engine, TOML
mod layer, WebSocket server, a terminal test client (with a `--bot`
autopilot for solo playtesting), and one Flutter client
(`clients/flutter`) covering both the desktop apps (Windows, Linux,
macOS) and the browser. Player accounts
are optional and verified against an external OIDC identity provider
(self-hosted, e.g. Rauthy - ADR-0009); guests can always play.

## Workspace

| Crate                | Layer (architecture doc)  | Contents                                             |
| -------------------- | ------------------------- | ---------------------------------------------------- |
| `parcello-engine`    | Game Engine (section 4)   | Pure, synchronous rules. No I/O, no async, no rand.  |
| `parcello-mods`      | Mod Layer (section 7)     | TOML bundles, Registry merge, `ModPlugin` trait.     |
| `parcello-protocol`  | Transport contract        | JSON message envelopes shared by server and clients. |
| `parcello-server`    | Transport + Session (5)   | Axum WS server, rooms, auth, history.                |
| `parcello-cli`       | Test harness              | Terminal client; `--bot` autopilot fills seats solo. |

Not a cargo crate: `clients/flutter` is the Dart/Flutter client - desktop
(Windows, Linux, macOS) and web from one codebase, with an OIDC login flow
on both (native loopback redirect on desktop, popup + postMessage on web,
ADR-0025). See `clients/flutter/README.md`.

Patterns from the doc and where they live:

- Command: `engine::command`, single `Engine::apply` pipeline (ADR-0001).
- Observer: `engine::event::Event`, broadcast by rooms after each command.
- State Machine: room `Lobby -> Active -> Finished` (`server::room`),
  turn `AwaitMove -> BlindAuction/BribeVote -> AwaitEnd`
  (`engine::state::TurnPhase`).
- Registry: `mods::RegistryBuilder` freezes into validated `GameContent`.
- Strategy: `RentCalculator`, `BankruptcyResolver` behind `dyn` in `Engine`
  (V2 WASM substitutes implementations here).
- Repository: `server::history::GameHistory` port with two adapters:
  in-memory (default) and SQLite via a writer thread (`--history`, ADR-0005).
- Plugin: `mods::ModPlugin` (`on_load`/`on_unload`); V1 ships the TOML
  implementation, V2 adds a Wasmtime-backed one behind the same trait.

## Quickstart

Rust 1.96+.

```sh
cargo build --workspace
cargo test  --workspace

# Server (guest auth, LAN/testing only)
cargo run -p parcello-server -- --insecure-guest
```

**Play in a browser:** `cd clients/flutter && flutter build web --release`,
point the server at the output (`--web-dir build/web`, see below), then
open `http://localhost:7878/`, enter a name, leave the code empty to
create a room (or paste a code to join), then click the room code to copy
it and share. Room codes are pronounceable (CVCVC, e.g. `GOLUR`) so they
are easy to read out over voice chat. This is the same Flutter codebase as
the desktop client (ADR-0025) - the server stays the only authority.

**Or with the terminal client:**

```sh
# Terminal 2: host
cargo run -p parcello-cli -- --name alice --create
# prints: room created: ABCDE

# Terminal 3: guest
cargo run -p parcello-cli -- --name bob --join ABCDE

# No players around? Fill seats with bots (autopilot: buy, bid, build,
# mortgage/redeem, boost, seize groups, handle jail/trades). Great for solo LAN/WAN testing.
cargo run -p parcello-cli -- --name bot1 --join ABCDE --bot

# In the host terminal:
#   start
# then per the prompts: play <n> | route <n,n,...> | bribe <amount> | vote yes|no
#                       | card | bid <n> | build <t> | sell <t> | mortgage <t>
#                       | redeem <t> | end | resign
# trading (any time):   offer <seat> <give$> <tiles|-> <want$> <tiles|->
#                       accept <id> | refuse <id> | cancel <id>
```

Server flags: `--bind 0.0.0.0:7878`, `--mods-dir mods`, `--mod base`
(repeatable, ordered; later mods override earlier ones per key),
`--web-dir web` (the built Flutter Web client, served at `/`; refuses to
start if it has no `index.html`, ADR-0025),
`--insecure-guest`, `--history <file.db>` (SQLite game logs; omit for
in-memory, see ADR-0005), `--turn-timeout <secs>` (auto-play the pending
canonical action - lowest hand card / ascending Legal Route / decline /
end turn - for a *connected* player who stalls that long, unless their
time bank still covers the overage;
default 12, 0 = disabled - a *disconnected* player is always skipped after a
30s grace regardless), `--time-bank-seconds <secs>` (personal per-match
reserve a connected player draws on to overrun the turn limit, never
refilled; default 45, 0 = off, ADR-0023), `--game-timeout <secs>`
(time-box games: at the buzzer the richest player by net worth wins,
ADR-0010; 0 = off), `--identity-url <jwks-url>`
(repeatable; accept EdDSA identity tokens from an OIDC provider such as
Rauthy, ADR-0009) with optional `--identity-audience <client-id>`.
`PARCELLO_JWT_SECRET` (HS256, ADR-0003) still works but is deprecated.
`--lan` announces the server on the LAN so clients can find it without a
URL (multicast `239.255.0.1:55888` by default, override with `--lan-maddr`
/ `--lan-port`; add `--lan-broadcast-fallback` for networks that block
multicast; ADR-0016). The Flutter client's "Browse public games" browses
these announcements.

Docker: `docker build -t parcello . && docker run -p 7878:7878 parcello`
(mount a volume and add `--history data/parcello.db` for persistence), or
pull the published image: `ghcr.io/vianpyro/parcello-server`. For a
ready-to-run local deployment with persistent history and editable server
settings, use `docker compose -f compose-example.yml up --build`.

Accounts are optional and only exist for continuity/stats: guests can
always play. The Flutter client has a "Sign in with account" button on
both desktop and web (OIDC + PKCE against your identity provider,
ADR-0025); only the CLI accepts a pasted token instead.

## Development & testing

The same checks CI enforces (`.github/workflows/ci.yml`), runnable locally:

```sh
# Rust: 136 tests, formatting, and lints (all must pass before a PR)
cargo test   --workspace --locked
cargo fmt    --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# MSRV: the project builds on Rust 1.96 with the committed lockfile
cargo build  --workspace --locked

# Optional but part of release hygiene: no vulnerable dependencies
cargo audit
```

Always pass `--locked` so builds are reproducible against the committed
`Cargo.lock`. Engine rules are covered in `crates/engine/tests/engine.rs`
(scripted movement via `PlayMovementCard`, `plain_board`/`transit_board`
fixtures); session behaviour (rooms, reconnect tokens, private trades,
feedback) has async tests in `crates/server/src/room.rs`; the wire format
is pinned by tests in `parcello-protocol`.

Flutter client (needs the Flutter SDK):

```sh
cd clients/flutter
flutter analyze
flutter test
```

**Play interactively as a dev** — run the Flutter client against a local
server, with hot reload for UI work:

```sh
# Terminal 1: server (guest auth, LAN/testing)
cargo run -p parcello-server -- --insecure-guest

# Terminal 2: the dev client (use -d linux or -d macos on other OSes)
cd clients/flutter
flutter run -d windows
```

Keep the default URL `ws://127.0.0.1:7878/ws`, enter a name, and leave the
room code empty to create a room (or paste one to join). The same client
built for web at `http://localhost:7878/` and the terminal client
(`parcello-cli --name you --create`) are lighter ways to take a seat.

**End-to-end with bots:** take one seat from a human client (above), then
fill the remaining seats with bots and watch a full game (0 rejected
commands is the bar). It also runs fully headless with a CLI as host:

```sh
cargo run -p parcello-server -- --insecure-guest --history game.db
cargo run -p parcello-cli -- --name host --create           # start the game
cargo run -p parcello-cli -- --name bot1 --join CODE --bot
cargo run -p parcello-cli -- --name bot2 --join CODE --bot
```

With `--history`, post-game survey answers land in the SQLite `feedback`
table, so you can verify the whole feedback path end-to-end.

## Releases

Bumping the workspace `version` in `Cargo.toml` on `main` triggers
`.github/workflows/release.yml`: it tags `vX.Y.Z`, builds the Flutter Web
client once and bundles it (with `mods/`) into the server + CLI tarballs
for Linux (x64 + arm64), Windows, and macOS (arm64), builds the Flutter
desktop client for Windows, Linux, and macOS, assembles Steam-depot-shaped
all-in-one archives (client + server together) for Windows and Linux (the
Linux one fits the Steam Deck), attaches everything to an auto-generated
GitHub release, and pushes the server image to GHCR (`vX.Y.Z` + `latest`,
linux/amd64 - it builds its own Flutter Web client in a self-contained
Docker stage, ADR-0025). Keep `clients/flutter/pubspec.yaml`'s version in
step - it stamps the client executable. Re-pushing without a bump is a
no-op. All dependency licenses are permissive (checked with cargo-license),
so commercial distribution is unencumbered.

## Protocol (v0, JSON over WebSocket at `/ws`)

Client -> server: `create {auth, mods?}` (optional ordered mod list for
the room, ADR-0006; omit for the server default), `join {code, auth}`,
`start`, `play_again` (after a game ends, restart it in the same room for
whoever is still connected; first sender wins), `leave` (leave the room but
keep the socket open, so the same connection can create/join another room),
`add_bot` / `remove_bot` (host, lobby: add or drop a server-driven bot
seat, ADR-0014), `configure {settings}` (host, lobby: set the room's timers
and rules, clamped server-side, ADR-0015), `cmd {cmd}`,
`feedback {rating, comment?}` (post-game survey: 1-5 plus an optional
comment, stored in the server history; one per player per game, fully
optional and never blocking), `animation_done {through_seq}` (this client
finished rendering every update through `through_seq`, ADR-0028 - the
server's animation-sensitive timers wait for these acks, bounded by a 6s
hard cap; clients with no animations send it immediately), `ping`.
Server -> client: `room_created`, `joined`
(includes the resolved mod bundle, a per-seat reconnect token - present it
in `auth.reconnect` to re-take a guest seat, ADR-0008 - and, mid-game, a
state snapshot), `lobby`,
`game_started`, `update {seq, events, view}` (`seq` is the monotonic
per-room counter the ack above refers to), `rejected {error}` (sent only to
the offending player), `error`, `pong`. Shapes live in `parcello-protocol`;
commands and events are the engine's own serialized types, so the wire
format is the replay format.

## Mods (V1, data-only)

A mod is a directory under `--mods-dir`:

```
mods/<id>/
  manifest.toml          # id, version, author, min_game_version
  data/properties.toml   # [[tile]] board definitions
  data/cards.toml        # [[chance]] / [[community]]
  data/rules.toml        # [rules] named scalar overrides
```

The default `mods/base` is the **32-tile fast board** (a 9x9 ring, no
Community Chest, two "utility" tiles - Wi-Fi and The Chatbot,
group-scaled like Monopoly's Water Works/Electric Company - instead of
four stations, two chance tiles, one tax tile; the design goal is fast,
dynamic games). Base is loaded first; merge is last-loaded-wins per
key: tiles and cards replace in place by id, rule scalars override by
name; every conflict is logged at WARN. Unknown rule keys are ignored
with a warning. The resolved bundle is pushed to clients on join, so
clients never need mod files locally.

Each room can pick its own ordered mod list at creation (ADR-0006): the
clients expose a "mods" field on create (CLI: repeatable `--mod`), and an
omitted or empty list selects the server's boot-time default set. Mod ids
are allowlist-validated server-side. Example: `mods/highroller`
(rules-only: richer, faster) with `base, highroller`.

V1 hook points: `rules.{starting_balance, go_salary, velocity_min,
velocity_max, max_houses_per_property, bankruptcy_threshold,
expropriation, rent_boost, win_full_groups, win_victory_points,
subsidiary_pool_factor, conglomerate_pool_factor, spotlight_rent_pct,
spotlight_duration_turns}` (booleans as 0/1;
`velocity_min`/`velocity_max` size the movement card hand (ADR-0017;
also the fixed permutation length for a Legal Route, ADR-0024),
`expropriation`/`rent_boost` are cost percents, `win_full_groups` a group
count, `win_victory_points` a point target (ADR-0020), the two pool
factors a multiplier scaling `round(factor * sqrt(players))`, 0 = off,
and `spotlight_rent_pct`/`spotlight_duration_turns` the Exposition
corner's rent bonus percent and window length in turns (ADR-0026, 0 =
off, see below), `cards.chance[*]`,
`cards.community[*]`, `properties[*]` (including per-tile `rent_model`:
`houses` (default) or `group_scaled` for stations; the scaled model needs
no `house_cost` and cannot be built on),
`events[*]` + `[forecast] gap_turns` (ADR-0021, see below).

## Game rules implemented

Movement is a velocity deck (ADR-0017): every player holds a public hand
of every integer in `rules.velocity_min..=velocity_max` (1-5 by default)
and plays one card per turn (`PlayMovementCard`), collecting Go salary on
passing/landing on Go; the hand refills to the full range the instant it
empties, and full refills are the metronome for the victory-point race's
round bonus (ADR-0020). Sealed-bid auctions on every landing (ADR-0018):
a 5s window opens the instant a player lands on an unowned property, and
every living seat - not just the landing player - submits exactly one bid
at once (`0` abstains); the landing player (the "discoverer") is treated
as bidding list price if they stay silent and can afford it, and an
explicit non-zero discoverer bid must meet that floor - landing on an
affordable tile always commits you to at least the floor, there is no
plain decline anymore. The window resolves the instant every living seat
has bid (or the server auto-abstains whoever's left silent at the
deadline): highest effective bid wins, ties go to the discoverer then the
lowest seat, and an all-zero result (only possible when the discoverer is
also broke) leaves the tile unsold. The discoverer pays list price in
full when winning at the floor, but only 90% (floored) of their bid when
winning above it after a contest; any other winner always pays their bid
in full. Bids are private while the window is open - a view shows only
your own - and revealed together once it resolves. Cash is frozen for the
whole window, same invariant as the old open auction. Trading
(asynchronous offers of cash and/or house-free-group tiles between any
solvent players, re-validated at acceptance so stale offers reject
without side effects; blocked while a sealed-bid window is open, to
preserve its cash-frozen invariant; capped at 4 open offers per proposer;
offers are private - only the two parties see them and their lifecycle
events), rent (full-group doubles
unimproved rent; a singleton group counts as full; stations scale by
stations owned), building and voluntary house
sales with the classic even-build/even-sell rule (forced liquidation
follows it too), the full-group requirement, per-tile cap, and
no-mortgage-in-group rule. Optional shared building pools (ADR-0019,
`rules.subsidiary_pool_factor`/`conglomerate_pool_factor`, 0 = unlimited,
6/3 in the base fast board): a table-wide stock of levels 1..max-1
("subsidiaries") and the top level ("conglomerates"), sized
`round(factor * sqrt(players))` at game start and public in every view;
`Build` draws from the matching pool and rejects (`pool_exhausted`) when
it's empty, building the top level converts one conglomerate and releases
the tile's subsidiaries back; stepping a tile back down off the top level
can be rejected the same way if the subsidiary pool can't re-issue them,
but forced (bankruptcy) liquidation always succeeds regardless, falling
back to a one-motion full strip when needed. Mortgage (half
price out, plus 10% interest floored to redeem; mortgaged tiles collect
nothing but still count for group ownership; a group must be house-free to
mortgage), taxes, chance/community decks (cyclic, seeded shuffle, chained
card moves capped at depth 4), jail entered the same way as before (the Go
To Jail tile or a card) but escaped by choice under the blitz clock, not
dice (ADR-0024): Legal Route (`ChooseLegalRoute`, commit to a locked,
public permutation of the full hand - the first card plays immediately
and un-jails you, each following turn only the route's front card is a
legal `PlayMovementCard`, and while any of the route remains your owned
tiles charge no rent to visitors; the hand refills normally, one
`hands_cycled` tick, once the route empties), Corruption (`OfferBribe`,
1..=cash, opens a 5s simultaneous vote among living opponents reusing
ADR-0018's timed-collection window; strictly more than half must accept -
a two-player game needs the lone opponent's yes; on success the amount
splits by floor division among the opponents, the remainder staying with
the briber, and you exit with a normal hand and live rents to play your
move the same turn; on failure no cash moves and the turn just ends,
retry next turn), and the unchanged get-out-of-jail-free card
(`UseJailCard`, held as a per-player count, immediate unconditional exit
then a normal move the same turn; the decks are immutable cyclic
shuffles, so drawn cards never leave the rotation). A jailed seat's
canonical/AFK action is the Legal Route in ascending order - nobody rots
in jail. Partial-payment bankruptcy with liquidation (houses at
half cost first, then automatic mortgages, highest value first) and asset
transfer to the creditor (mortgages carry over as-is; the bank refurbishes
returned tiles), resignation, last-player-standing win. Optional time-boxed games
(`--game-timeout`): at the buzzer the richest player by net worth wins
(cash + property equity + houses), ADR-0010. Aggressive mods-gated
mechanics for swingy games: expropriation (seize a rival's property at a
premium, landing tile only, right after rent, at the end of your turn; the
owner is compensated, ADR-0011, tightened by ADR-0022 - improved tiles are
seizable too, their buildings liquidating at half cost to the former owner
on top of the usual compensation and returning to the shared pools) and
rent boosts
(pay to raise an owned tile's rent one step, capped, ADR-0012) - both on by
default in the base fast board. Multiple win conditions: last player
standing, richest at the time limit (ADR-0010), a domination win - control
N complete colour groups (`rules.win_full_groups`, off by default in the
base fast board, ADR-0013) - and the primary v2 win condition, a race to
`rules.win_victory_points` (20 in the base fast board, ADR-0020):
`PlayerView.victory_points` is `3` per complete colour group, `2` per
conglomerate-level tile, `1` per group-scaled ("utility") tile owned, plus
a stored `+2`/round bonus that permanently banks to whoever has the
strictly highest cash each time every surviving player has completed a
turn (ties to the lowest seat) - everything but the round bonus is a live
recomputation of the current board, so a hostile takeover (ADR-0022) both
gains and costs points in the same instant. Reaching the target ends the
game instantly (`Event::WonByPoints`); if nobody gets there first and a
`Build` empties the shared conglomerate pool (ADR-0019), the game ends
immediately too - highest score wins, ties by net worth then the lowest
seat (`Event::WonByPoolExhaustion`, the "doom clock"). Public market
forecast (ADR-0021, `data/events.toml`):
a seeded, rolling queue of the next 3 scheduled market events plus whichever
one is currently active - `rent_multiplier` (composes with the rent boost
step above), `acquisition_multiplier` (scales takeover cost), and
`wealth_tax` (one-shot: every alive player pays a percent of net worth
through the normal bankruptcy machinery). The whole queue is public in every
view - the draws already made, never the generator (seed/deck order stay
hidden) - so players can plan around it; `gap_turns` apart, chained,
deterministic from the game seed. The base mod ships a starter pool (market
bubble / crash / tax audit) with deliberately rough numbers - calibration is
a playtest task, never an engine change. The Exposition corner (ADR-0026,
replaces the old no-op Free Parking): landing there draws one random
property tile via the seeded RNG and puts it in the spotlight for
`spotlight_duration_turns` turns, boosting its rent by `spotlight_rent_pct`
- composes multiplicatively with the rent boost and market forecast steps
above. Unlike the rent boost, the spotlight is a fact about the tile, not
the owner: it survives a trade, expropriation, or bankruptcy transfer of
the spotlit property untouched. Landing on the corner again re-rolls and
replaces whichever tile was previously spotlit.

Deliberate V1 simplifications: no immediate interest charge when mortgaged
tiles change hands (trades and bankruptcy transfer them as-is);
get-out-of-jail cards are a count, not tradeable objects, and stay in the
deck rotation once drawn.

## Known MVP limitations

- Rooms with no connected seat dissolve after 30 minutes idle; there is no
  persistence, so a dissolved game is gone.
- Guest identities are spoofable at first join (`--insecure-guest`);
  mid-game seats are protected by reconnect tokens (ADR-0008).
- History is in-memory unless `--history` is set; the SQLite adapter logs
  `(seed, ordered accepted commands)`, i.e. complete deterministic replays.
- A guest who loses their reconnect token cannot re-take their seat until
  the room dissolves.
- A disconnected player's turn is auto-played after a 30s grace so an AFK
  player never stalls the table (they keep their seat and can rejoin). The
  room's turn limit (default 12s, host-editable, ADR-0015/0023) extends this
  to connected-but-idle players: a strict auto-skip of the acting seat, unless
  they still have personal time bank left (default 45s, a per-match reserve
  that is never refilled and does not apply to a disconnected seat, ADR-0023)
  - the overage is drained from the bank instead of an immediate auto-skip.
  Set the turn limit to off in the lobby if you want a present-but-slow
  player never forced. When on, `GameStarted`/`Joined` carry `turn_seconds`
  and `time_bank_seconds`, and clients show a per-turn countdown flowing into
  the bank (reset on each accepted command); absent when off.
- The host can add bots from the lobby (an "Add bot" button; `addbot` in the
  CLI). Bots are server-driven seats that play the shared autopilot
  heuristic at ~0.8s/move. They fill empty seats but yield to humans: a
  player joining a full room evicts the newest bot instead of being turned
  away (ADR-0014). Removed via "Remove bot" / `rmbot`.
- The host sets each game's options in the lobby (ADR-0015): the three
  timers (game, turn, time bank) and every rule scalar (starting balance, GO
  salary, velocity min/max, max houses, bankruptcy threshold, expropriation %,
  rent boost %, domination groups, victory point target,
  subsidiary/conglomerate pool factors). Edits broadcast live to the
  lobby; the server clamps every value. New rooms default to a 60-minute
  game with a 12 s turn limit and a
  45 s personal time bank (ADR-0023). One server runs many rooms, each with
  its own settings - no orchestrator needed. `--turn-timeout` /
  `--time-bank-seconds` / `--game-timeout` set the per-room defaults (0
  disables); the host overrides them. CLI: `set <field> <value>` (e.g.
  `set game 45`, `set turn off`, `set bank 60`, `set expropriation 0`,
  `set subsidiary_pool 6`).

## Deviations from the architecture doc

See `docs/adr/`: 0001 `apply` returns `Result`; 0002 PRNG seed inside
`GameState`; 0003 interim auth (guest + HS256 behind `IdentityVerifier`);
0004 server-wide mod set (room `Starting` state collapses to a point);
0005 rusqlite writer thread instead of SQLx behind `GameHistory`;
0006 per-room mod sets at creation (amends 0004, `Starting` stays
collapsed); 0007 private trade offers via per-seat `ClientView`s;
0008 per-seat reconnect tokens (guest seat hijack protection);
0009 Identity Service design (EdDSA JWT + JWKS, self-hosted and
redundant, accounts always optional); 0010 time-boxed games end by net
worth (server clock, engine rule); 0011 expropriation; 0012 rent boosts;
0013 domination win (control N full colour groups); 0014 server-side bot
seats (host-added, yield to humans, shared `bot::decide` heuristic);
0015 per-room host-editable settings (timers + rules chosen in the lobby,
clamped server-side; one server runs many independent games); 0018
sealed-bid auctions on every landing (replaces buy/decline and the open
round-robin auction; a server-timed 5s window collected from every living
seat at once, not a single actor - the first simultaneous multi-seat
command phase, and the model for ADR-0024's corruption vote); 0019 shared
building pools (subsidiaries/conglomerates, table-wide scarcity scaled by
player count); 0020 victory points + pool-exhaustion end (the primary v2
win condition; the round bonus reads `Player.hands_cycled`, ticked once
per hand refill by ADR-0017's velocity deck); 0021 public market
forecast (seeded, rolling event queue; rent/acquisition multipliers and a
one-shot wealth tax); 0022 takeover tightened to the landing tile only,
at end of turn, and improved tiles seizable with liquidation to the pools
(amends 0011, shares accounting with 0019); 0023 blitz clock: 12s turns
plus a 45s personal time bank, never refilled (amends 0015); 0017 velocity
deck replaces dice movement (a public per-player hand of
`rules.velocity_min..=velocity_max`, `PlayMovementCard`; `DicePolicy` and
`RentModel::DiceScaled` removed outright, `mods/classic` deleted as its
only user); 0024 jail escape without dice - Legal Route, Corruption, and
the unchanged jail card replace doubles/fine/third-roll (amends the entry
side of nothing - Go To Jail is unchanged, only escape is redesigned).

## Roadmap

- FX + audio and real multiplayer playtesting for the Flutter client
  (next priority).
- Deploy the OIDC issuer (Rauthy); the server-side EdDSA verifier is done
  (ADR-0009). The deprecated HS256 auth stays until LAN/WAN playtests have
  happened.
- Structured multiple-choice post-game surveys (today's survey is a single
  1-5 rating plus an optional comment).
- Android / mobile targets (postponed).
- WASM (Wasmtime) mod plugins behind `ModPlugin` (unblocked by the move to
  Rust 1.96).
- Richer history queries (stats) if needed; a Steam all-in-one release.
