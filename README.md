# Parcello

Open-source multiplayer board game in the spirit of Business Tour: fast,
dynamic games rather than Monopoly's slow accumulation. Authoritative Rust
server, thin clients, community-hosted servers (Minecraft model),
data-driven mods. The current rules are still Monopoly-close; the target
design and the gap are tracked in `docs/business-tour-direction.md`.

This repository holds the complete, playable game: pure game engine, TOML
mod layer, WebSocket server with an embedded browser client, a terminal
test client (with a `--bot` autopilot for solo playtesting), and a
cross-platform Flutter desktop client (`clients/flutter`). Player accounts
are optional and verified against an external OIDC identity provider
(self-hosted, e.g. Rauthy - ADR-0009); guests can always play.

## Workspace

| Crate                | Layer (architecture doc)  | Contents                                             |
| -------------------- | ------------------------- | ---------------------------------------------------- |
| `parcello-engine`    | Game Engine (section 4)   | Pure, synchronous rules. No I/O, no async, no rand.  |
| `parcello-mods`      | Mod Layer (section 7)     | TOML bundles, Registry merge, `ModPlugin` trait.     |
| `parcello-protocol`  | Transport contract        | JSON message envelopes shared by server and clients. |
| `parcello-server`    | Transport + Session (5)   | Axum WS server, rooms, auth, history, web client.    |
| `parcello-cli`       | Test harness              | Terminal client; `--bot` autopilot fills seats solo. |

Not a cargo crate: `clients/flutter` is the Dart/Flutter desktop client
(Windows, Linux, macOS), mirroring the web client feature-for-feature with
an added OIDC login flow. See `clients/flutter/README.md`.

Patterns from the doc and where they live:

- Command: `engine::command`, single `Engine::apply` pipeline (ADR-0001).
- Observer: `engine::event::Event`, broadcast by rooms after each command.
- State Machine: room `Lobby -> Active -> Finished` (`server::room`),
  turn `AwaitRoll -> AwaitBuy -> AwaitEnd` (`engine::state::TurnPhase`).
- Registry: `mods::RegistryBuilder` freezes into validated `GameContent`.
- Strategy: `DicePolicy`, `RentCalculator`, `BankruptcyResolver` behind
  `dyn` in `Engine` (V2 WASM substitutes implementations here).
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

**Play in a browser:** open `http://localhost:7878/`, enter a name, leave
the code empty to create a room (or paste a code to join), share the code.
The client is a single embedded HTML file speaking the same protocol as the
CLI; the server stays the only authority. It has not been exercised in a
real browser inside the development sandbox (protocol coverage is verified
mechanically against the engine's command and event enums), so expect
cosmetic rough edges.

**Or with the terminal client:**

```sh
# Terminal 2: host
cargo run -p parcello-cli -- --name alice --create
# prints: room created: ABCDE

# Terminal 3: guest
cargo run -p parcello-cli -- --name bob --join ABCDE

# No players around? Fill seats with bots (simple autopilot: buy, bid,
# build, jail cards; declines trades). Great for solo LAN/WAN testing.
cargo run -p parcello-cli -- --name bot1 --join ABCDE --bot

# In the host terminal:
#   start
# then per the prompts: roll | buy | no | bid <n> | pass | build <t>
#                       | sell <t> | mortgage <t> | redeem <t> | pay | end | resign
# trading (any time):   offer <seat> <give$> <tiles|-> <want$> <tiles|->
#                       accept <id> | refuse <id> | cancel <id>
```

Server flags: `--bind 0.0.0.0:7878`, `--mods-dir mods`, `--mod base`
(repeatable, ordered; later mods override earlier ones per key),
`--insecure-guest`, `--history <file.db>` (SQLite game logs; omit for
in-memory, see ADR-0005), `--turn-timeout <secs>` (auto-play the pending
canonical action - roll/decline/pass/end turn - for a *connected* player
who stalls that long; 0 = disabled, the default - a *disconnected* player
is always skipped after a 30s grace regardless), `--game-timeout <secs>`
(time-box games: at the buzzer the richest player by net worth wins,
ADR-0010; 0 = off), `--identity-url <jwks-url>`
(repeatable; accept EdDSA identity tokens from an OIDC provider such as
Rauthy, ADR-0009) with optional `--identity-audience <client-id>`.
`PARCELLO_JWT_SECRET` (HS256, ADR-0003) still works but is deprecated.

Docker: `docker build -t parcello . && docker run -p 7878:7878 parcello`
(mount a volume and add `--history data/parcello.db` for persistence), or
pull the published image: `ghcr.io/vianpyro/parcello-server`.

Accounts are optional and only exist for continuity/stats: guests can
always play. The Flutter client has a "Sign in with account" button
(OIDC + PKCE against your identity provider); the web client and CLI
accept a pasted token.

## Development & testing

The same checks CI enforces (`.github/workflows/ci.yml`), runnable locally:

```sh
# Rust: 62 tests, formatting, and lints (all must pass before a PR)
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
(scripted dice via `FixedDice`, `plain_board`/`transit_board` fixtures);
session behaviour (rooms, reconnect tokens, private trades, feedback) has
async tests in `crates/server/src/room.rs`; the wire format is pinned by
tests in `parcello-protocol`.

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
room code empty to create a room (or paste one to join). The embedded
browser client at `http://localhost:7878/` and the terminal client
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
`.github/workflows/release.yml`: it tags `vX.Y.Z`, builds the server + CLI
for Linux (x64 + arm64), Windows, and macOS (arm64) with the `mods/`
directory bundled, builds the Flutter client for Windows, Linux, and
macOS, assembles Steam-depot-shaped all-in-one archives (client + server
together) for Windows and Linux (the Linux one fits the Steam Deck),
attaches everything to an auto-generated GitHub release, and pushes the
server image to GHCR (`vX.Y.Z` + `latest`, linux/amd64). Keep
`clients/flutter/pubspec.yaml`'s version in step - it stamps the client
executable. Re-pushing without a bump is a no-op. All dependency licenses
are permissive (checked with cargo-license), so commercial distribution
is unencumbered.

## Protocol (v0, JSON over WebSocket at `/ws`)

Client -> server: `create {auth, mods?}` (optional ordered mod list for
the room, ADR-0006; omit for the server default), `join {code, auth}`,
`start`, `cmd {cmd}`, `feedback {rating, comment?}` (post-game survey:
1-5 plus an optional comment, stored in the server history; one per
player per game, fully optional and never blocking), `ping`. Server -> client: `room_created`, `joined`
(includes the resolved mod bundle, a per-seat reconnect token - present it
in `auth.reconnect` to re-take a guest seat, ADR-0008 - and, mid-game, a
state snapshot), `lobby`,
`game_started`, `update {events, view}`, `rejected {error}` (sent only to
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
Community Chest, two "resorts" instead of four stations; the design goal is
fast, dynamic games). `mods/classic` preserves the 40-tile Monopoly-like
long game for players who want it. Base is loaded first; merge is
last-loaded-wins per key: tiles and cards replace in place by id, rule
scalars override by name; every conflict is logged at WARN. Unknown rule
keys are ignored with a warning. The resolved bundle is pushed to clients
on join, so clients never need mod files locally.

Each room can pick its own ordered mod list at creation (ADR-0006): the
clients expose a "mods" field on create (CLI: repeatable `--mod`), and an
omitted or empty list selects the server's boot-time default set. Mod ids
are allowlist-validated server-side. Examples: play the long game with
`--mod classic`, or `mods/highroller` (rules-only: richer, faster) with
`base, highroller`.

V1 hook points: `rules.{starting_balance, go_salary, jail_fine,
max_houses_per_property, bankruptcy_threshold, auction_on_decline,
expropriation, rent_boost, win_full_groups}` (booleans as 0/1;
`expropriation`/`rent_boost` are cost percents and `win_full_groups` a
group count, 0 = off), `cards.chance[*]`,
`cards.community[*]`, `properties[*]` (including per-tile `rent_model`:
`houses` (default), `group_scaled` for stations, `dice_scaled` for
utilities; the scaled models need no `house_cost` and cannot be built on).

## Game rules implemented

Movement with Go salary, property purchase offers, trading (asynchronous
offers of cash and/or house-free-group tiles between any solvent players,
re-validated at acceptance so stale offers reject without side effects;
blocked during auctions to preserve the auction's solvency invariant;
capped at 4 open offers per proposer; offers are private - only the two
parties see them and their lifecycle events), auctions on declined
purchases (round-robin left of the decliner, strict raises capped by cash,
high bidder skipped until outbid, no bids leaves the tile unsold; disable
with `rules.auction_on_decline = 0`), rent (full-group doubles
unimproved rent; a singleton group counts as full; stations scale by
stations owned, utilities by dice total), building and voluntary house
sales with the classic even-build/even-sell rule (forced liquidation
follows it too), the full-group requirement, per-tile cap, and
no-mortgage-in-group rule, mortgage (half
price out, plus 10% interest floored to redeem; mortgaged tiles collect
nothing but still count for group ownership; a group must be house-free to
mortgage), taxes, chance/community decks (cyclic, seeded shuffle, chained
card moves capped at depth 4), jail (doubles escape, fine, forced fine on
the third failed roll), get-out-of-jail-free cards (held as a per-player
count, spent voluntarily before rolling or automatically instead of the
forced third-roll fine; the decks are immutable cyclic shuffles, so drawn
cards never leave the rotation), doubles grant an extra roll and three consecutive
doubles jail you, partial-payment bankruptcy with liquidation (houses at
half cost first, then automatic mortgages, highest value first) and asset
transfer to the creditor (mortgages carry over as-is; the bank refurbishes
returned tiles), resignation, last-player-standing win. Optional time-boxed games
(`--game-timeout`): at the buzzer the richest player by net worth wins
(cash + property equity + houses), ADR-0010. Aggressive mods-gated
mechanics for swingy games: expropriation (seize a rival's unimproved
property at a premium; the owner is compensated, ADR-0011) and rent boosts
(pay to raise an owned tile's rent one step, capped, ADR-0012) - both on by
default in the base fast board. Multiple win conditions: last player
standing, richest at the time limit (ADR-0010), and a domination win -
control N complete colour groups (`rules.win_full_groups`, 3 in the base
fast board, ADR-0013).

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
  `--turn-timeout` flag extends this to connected-but-idle players; it is
  off by default, so a present but slow player is never forced.

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
0013 domination win (control N full colour groups).

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
