# CLAUDE.md

Parcello: open-source multiplayer board game. Design goal is Business-Tour
style - fast, dynamic games, NOT Monopoly's slow accumulation. The v2
ruleset that closes the gap is DONE - ADRs 0017-0024, summary and build
order (complete) in `docs/business-tour-direction.md` (read both before
touching rules). Authoritative Rust server, thin clients, community-hosted
servers (Minecraft model), data-driven TOML mods. This repo is the
complete, playable backend V1: pure engine, mod layer, WebSocket server,
terminal test client, SQLite history. The one client (`clients/flutter`)
covers desktop and web from a single Dart codebase (ADR-0025) - the server
serves the Flutter Web build itself, from a runtime `--web-dir`.

Authoritative documents, in order of precedence:
1. `docs/architecture.typ` - the design document (game vision, layer rules,
   required patterns). Any deviation from it REQUIRES a new ADR in
   `docs/adr/` (short: context / decision / consequences).
2. `docs/adr/0001..0024` - accepted deviations. Read them before touching
   the engine, auth, mods, or history. Do not silently contradict them.
   0017-0024 are the v2 ruleset (implemented).
3. `README.md` - user-facing behavior reference (rules implemented, flags,
   protocol summary, known limitations).

## Hard constraints (do not break)

- **MSRV is Rust 1.96** (`rust-version` in the workspace; tracks recent
  stable since the 2026-07 dependency refresh - the old 1.75 pins are
  gone). Keep the CI msrv job, the Dockerfile base image, and
  `rust-version` in step when bumping. Consequences:
  - Build/test with `--locked` (reproducibility); `cargo update` is
    allowed but run `cargo test --workspace` and `cargo audit` after.
  - `cargo audit` is part of the release hygiene: the tree was clean at
    the refresh; keep it that way.
- **Engine purity** (crate `parcello-engine`): no I/O, no async, no rand,
  no clock. Randomness comes only from the SplitMix64 state inside
  `GameState.rng` (ADR-0002). Deps: serde + thiserror only.
- **Rejections never mutate** (ADR-0001): `Engine::apply` returns
  `Result<(GameState, Vec<Event>), CommandError>`; on `Err` the input state
  is untouched. The accepted-command log is therefore a complete
  deterministic replay: `(initial players, seed, ordered accepted commands)`
  must replay bit-identically. The test
  `same_seed_produces_identical_games` guards this; extend it when adding
  phases (it drives canonical actions derived from `TurnPhase`).
- **`ClientView` must never expose** `GameState.rng` or deck order
  (chance/community draws and market events would become predictable).
  Cash is public by design; trade offers are visible only to their two
  parties (ADR-0007) - the server sends `ClientView::for_seat` views,
  never the omniscient `of`.
- **Auction solvency invariant**: cash cannot change while
  `TurnPhase::BlindAuction` is active. Bids are validated against cash at bid
  time, so the winner can always pay at settlement. This is WHY all four
  trade commands are rejected during auctions - keep it that way, and keep
  the invariant if you add any new cash-moving command.
- **Even-build/even-sell** applies everywhere houses move: `Build`,
  `SellHouse`, AND `StandardLiquidation`. If you touch one, keep the three
  consistent.

## Commands

```sh
cargo build --workspace --locked
cargo test  --workspace --locked          # 133 tests, all must pass
cargo run -p parcello-server -- --insecure-guest [--history game.db]
# Browser client: http://localhost:7878/   (create/join by 5-letter code;
#   codes are pronounceable CVCVC, `random_code` in room.rs, click to copy)
cargo run -p parcello-cli -- --name alice --create
cargo run -p parcello-cli -- --name bob --join ABCDE
```

`.devcontainer/` ships the full toolchain in a container (Rust stable +
pinned 1.96 for MSRV checks, Flutter + Linux desktop, docker CLI against
the host daemon, cargo-audit/cargo-license, typst). Windows/macOS client
artifacts and Steam packaging remain CI's job (release.yml).

CI (`.github/workflows/ci.yml`): stable job runs `fmt --check`,
`clippy --all-targets --locked -- -D warnings`, tests; msrv job builds on
1.96 with `--locked`.

Releases (`.github/workflows/release.yml`): bumping the workspace version
in `Cargo.toml` on main tags `vX.Y.Z` and publishes a GitHub release with
server+CLI binaries (linux x64/arm64, windows, macos arm64; `mods/`
bundled), Flutter client bundles (windows/linux/macos), all-in-one
archives for windows and linux (client + server, Steam-depot-shaped; the
linux one also fits the Steam Deck), and a GHCR image
(`ghcr.io/<owner>/parcello-server`, amd64). Keep the pubspec version in
step. The release goes live only after all binary jobs succeed
(draft-then-publish); the docker job is independent. Dependency licenses
are all permissive (checked 2026-07 with cargo-license; keep it that way
- commercial distribution is planned).

Self-hosting: `compose-deploy.yml` + `.env.example` deploy the game
server with a Rauthy issuer behind any reverse proxy (`.env` is
gitignored - it holds secrets); `docs/deployment.md` is the guide (NPM +
Cloudflare, parallel community servers sharing the main identity,
independent servers, guest LAN mode). `compose-example.yml` stays the
local build-from-source variant.

## Architecture map

Workspace crates and their single responsibility (strict layering,
architecture doc section 5; dependencies point downward only):

- `crates/engine` - pure synchronous rules. `lib.rs` wires strategies
  (`RentCalculator`, `BankruptcyResolver` as `Box<dyn>`);
  `apply.rs` is the whole command pipeline (validate -> mutate clone ->
  emit events); `state.rs` (GameState, TurnPhase incl. `BlindAuction` and
  `BribeVote` variants, TradeOffer), `content.rs` (GameContent, RentModel),
  `view.rs`. `bot.rs`
  is the shared autopilot heuristic (`bot::decide(content, view, seat) ->
  Option<CommandKind>`): pure like everything here, used by both the
  server's bot seats and the CLI `--bot` (ADR-0014).
- `crates/mods` - TOML mod bundles. `RegistryBuilder` merges
  last-loaded-wins per key (tiles/cards replace in place by id, rule
  scalars override; conflicts logged WARN). Base game content is itself a
  mod (`mods/base/`). `resolve()` -> `ResolvedContent` (pushed verbatim to
  joining clients: mod distribution MVP).
- `crates/protocol` - JSON envelopes (`ClientMessage`/`ServerMessage`).
  Commands/events on the wire ARE the engine's serde types (externally
  tagged, snake_case) - the wire format is the replay format. Wire-format
  tests exist; changing serde shapes is a protocol break.
- `crates/server` - axum. `ws.rs` (transport: parse, authenticate once at
  create/join, relay; identity is bound to the connection and never
  re-trusted from the wire), `room.rs` (one Tokio task per room; state
  machine Lobby -> Active -> Finished; `PlayAgain` restarts a Finished room
  for the still-connected seats via the shared `start_game`; `Leave` drops
  the room but keeps the socket open (ws.rs clears the session so the same
  connection can create/join again - the Flutter client's connect/menu
  split relies on this); host = seat 0; 2..=6 players; rejoin
  by identity, last connection wins, but spoofable (guest) seats require
  the per-seat reconnect token issued in `Joined` (ADR-0008); host
  `AddBot`/`RemoveBot` (lobby only) add server-driven bot seats (`is_bot`,
  synthetic `bot:N` identity, `tx: None`) that the room task plays via
  `next_bot_action` -> `bot::decide` at `BOT_THINK` = 800ms/move; bots
  yield to humans - joining a full room evicts the newest bot instead of
  rejecting; `SeatInfo.is_bot` labels them (ADR-0014); rooms with
  zero connected seats
  dissolve after `IDLE_TIMEOUT` = 30 min; per-room settings
  (`RoomSettings` = timers + full `RuleParams`, ADR-0015): host edits them in
  the lobby via `Configure` (host+lobby only, `clamp_settings` bounds the
  untrusted wire values), broadcast on every `Lobby`/`Joined`; `start_game`
  rebuilds the engine with the effective rules and derives the timers from
  the settings, so the config is frozen and replay-safe once Active; smart
  per-turn AFK timer (`afk_deadline`, recomputed each loop so a mid-turn
  disconnect shortens it): a disconnected acting seat is auto-played the
  canonical action (lowest hand card / ascending Legal Route / EndTurn,
  ADR-0017/0024 - `BlindAuction`/`BribeVote` have no single actor, so
  `acting_seat` excludes them and their own parallel `bid_deadline`/
  `vote_deadline` timers auto-abstain/auto-reject silent seats instead)
  after `DISCONNECTED_GRACE` = 30s
  always, a connected-but-idle seat when `settings.turn_seconds` is set
  (default 25s, `--turn-timeout` sets the per-room default, 0 = off); any
  accepted command resets the clock; game clock derived from
  `settings.game_seconds` (default 3600s, `--game-timeout` default, 0 =
  untimed) ends a time-boxed game via `Engine::finish_on_time` - richest by
  `GameState::net_worth` wins, ties to lowest seat, `Event::TimeUp`
  (ADR-0010); `GameStarted`/`Joined` carry `time_remaining` for the game
  countdown (clients mirror the net-worth formula) and `turn_seconds` for a
  per-turn countdown the clients reset on each Update; post-game
  survey `feedback` message:
  Finished phase only, once per seat, rating 1-5 + comment capped at 500
  chars, stored via `GameHistory::record_feedback` - the client UI must
  stay non-blocking, side card not modal), `auth.rs` + `eddsa.rs`
  (`IdentityVerifier` trait: insecure guests, EdDSA identity tokens
  verified against JWKS from any OIDC provider - Rauthy is the reference,
  ADR-0009 - and the deprecated HS256 stopgap, ADR-0003; tokens dispatch
  on the header `alg`), `history.rs` (`GameHistory` port; in-memory
  adapter + `SqliteHistory`: dedicated writer thread owns the rusqlite
  connection, trait methods enqueue and never block, `Drop` drains -
  ADR-0005), `lan.rs` (opt-in `--lan` UDP discovery announcer: periodic
  multicast to `239.255.0.1:55888` with optional broadcast fallback so LAN
  clients find the server without a URL; best-effort, detached, no admin
  control plane - local process management is the client's job, ADR-0016),
  the Flutter Web client (served from disk at runtime via `tower-http`'s
  `ServeDir`, `--web-dir`/`PARCELLO_WEB_DIR`, default `web` - mirrors
  `--mods-dir`'s pattern, not compiled into the binary; fails loudly at
  boot if the directory has no `index.html`, same idiom as
  `parcello_mods::resolve`'s `?` propagation - ADR-0025).
- `crates/cli` - terminal test harness; keep it in sync with new commands
  (it is the cheapest end-to-end protocol check; `addbot`/`rmbot` and
  `set <field> <value>` stdin commands too, ADR-0015; the `discover` bin
  is a headless listener to validate `--lan` announcements). `--bot` turns it into an autopilot seat using the shared
  `parcello_engine::bot::decide` (bid/build/jail card > bribe > Legal
  Route, declines trades) so games can be playtested without volunteers;
  soak it with 3 bots when touching turn flow. Server-side bots
  (ADR-0014) reuse the same heuristic.
- `clients/flutter` - Flutter client, one codebase for desktop (Windows,
  Linux, macOS) and web (ADR-0025; Dart, not part of the cargo workspace).
  See its README. Requires the Flutter SDK (`flutter analyze && flutter
  test`, `flutter build web --release` for the web target). Four files
  differ per platform via conditional export (`dart.library.js_interop`),
  since `dart:io` doesn't exist on web: `oidc.dart` (system browser +
  loopback redirect on desktop vs. popup + `postMessage` on web),
  `lan_discovery.dart` and `server_manager.dart` (native-only, no browser
  equivalent - stubbed out on web, hidden behind `kIsWeb` in the menu),
  `session_storage.dart` (a file on desktop, `localStorage` on web). When
  adding an Event or CommandKind, update it too (protocol.dart +
  main.dart), same drill as the CLI.

Mods: the server resolves a default set at boot (`--mod`), and each room
may override it at creation via the optional `mods` field on Create
(ADR-0006; ids are allowlist-validated in `ws.rs` because they become
filesystem paths). Default `mods/base` is the 32-tile fast board (9x9
ring, no Community Chest, two "utility" tiles (Wi-Fi, The Chatbot -
group-scaled, modern reinterpretations of the original resort idea), two
chance tiles, one tax tile, `docs/business-tour-direction.md`);
`mods/highroller` is a rules-only example. Clients render any `4*(d-1)`
square ring (32, 40, ...); other tile counts fall back to a wrap layout.

## Game rules snapshot (what exists)

Movement is a velocity deck, no dice (ADR-0017): `PlayMovementCard` plays
a value from a public `Player.hand`, refilled to
`rules.velocity_min..=velocity_max` the instant it empties - that refill
also ticks `Player.hands_cycled`, ADR-0020's round metronome; plus Go
salary. Sealed-bid auctions on every landing (ADR-0018): a 5s
`TurnPhase::BlindAuction` window, every living seat bids at once via
`SubmitBlindBid` (0 abstains), the discoverer gets an implicit list-price
floor bid if silent and solvent, wins at the floor pay full price, wins
above it after a contest pay 90% floored, ties favour the discoverer then
the lowest seat, an all-zero result leaves the tile unsold - no plain
decline any more. Rent models per tile (`houses` default with full-group
x2 unimproved - a singleton group counts as full; `group_scaled` stations
- scaled models reject Build). Build/sell with the even rule (forced
liquidation follows it too). Shared building pools (ADR-0019,
`rules.subsidiary_pool_factor`/`conglomerate_pool_factor`,
`round(factor * sqrt(players))`, 0 = unlimited): `Build` draws from the
matching pool and rejects when empty, the top level converts a
conglomerate and releases subsidiaries, forced (bankruptcy) liquidation
always succeeds, falling back to a one-motion full strip when the pool
can't cover a normal step-down. Mortgage (price/2 out, +10% floored to
redeem, house-free group required, mortgaged tiles pay nothing but count
for ownership); taxes; cyclic seeded decks, card chains capped at depth
4. Jail is entered unchanged (Go To Jail tile/card) but escaped by choice
under the blitz clock, not dice (ADR-0024): Legal Route
(`ChooseLegalRoute`, a locked public permutation of the full hand - the
first card plays in the same command, un-jailing immediately; each
following turn only the route's front card is a legal
`PlayMovementCard`; while any of it remains, the route holder's tiles
charge no rent to visitors; the hand refills normally, one
`hands_cycled` tick, once the route empties), Corruption (`OfferBribe`,
`1..=cash`, opens `TurnPhase::BribeVote` - a 5s simultaneous vote among
living opponents reusing the ADR-0018 timed-collection-window pattern
with its own parallel `vote_deadline`, not a shared primitive; strictly
more than half must accept; on success the amount splits by floor
division among the opponents, remainder stays with the briber, briber
exits with a normal hand and live rents; on failure no cash moves, the
turn just ends, retry next turn), and the unchanged jail card
(`UseJailCard`, per-player count `jail_cards`, immediate unconditional
exit then a normal `PlayMovementCard`; cards stay in the cyclic deck once
drawn - a count, not tradeable objects). A jailed seat's canonical/AFK
action is the Legal Route in ascending order. Partial-payment bankruptcy
with even-aware liquidation (houses then auto-mortgages) and transfer to
creditor (mortgages carry as-is; bank refurbishes); **trading**
(asynchronous offers, any solvent player any time EXCEPT during a
`BlindAuction`/`BribeVote`; exempt from turn check like Resign;
re-validated at acceptance - stale offers reject without mutation; purged
on bankruptcy; max 4 open per proposer; offers and their lifecycle events
are private to the two parties, ADR-0007); resign; win conditions:
last-player-standing, richest at the time limit (`--game-timeout`,
ADR-0010), domination (`rules.win_full_groups` complete groups, ADR-0013,
off by default so it doesn't short-circuit the race), and the primary v2
condition - a race to `rules.win_victory_points` (ADR-0020, 20 in base:
3/complete colour group, 2/conglomerate-level tile, 1/group-scaled
("utility") tile owned, plus a stored, non-reversible `+2`/round bonus to
whoever has the strictly highest cash each time every surviving player
has completed a hand refill, ties to the lowest seat; reaching the target
ends the game instantly, `Event::WonByPoints`; if a `Build` empties the
conglomerate pool first, the game ends immediately too - highest score
wins, ties by net worth then lowest seat, `Event::WonByPoolExhaustion`,
the "doom clock"). Optional expropriation (`rules.expropriation`, seize a
rival's property at a premium, landing tile only, end of turn; improved
tiles liquidate to the shared pools, owner compensated, ADR-0011/0022)
and rent boosts (`rules.rent_boost`, +50%/step, cap 3, reset on transfer,
ADR-0012) - both on in the base fast mod. Public market forecast
(ADR-0021, `data/events.toml`): a seeded rolling queue of the next 3
scheduled events plus whichever is active - `rent_multiplier`,
`acquisition_multiplier`, one-shot `wealth_tax` - `gap_turns` apart,
public in every view (draws already made, never the generator). The
Exposition corner (`TileKind::Spotlight`, ADR-0026, replaces the old
no-op `free_parking` in `mods/base`): landing there draws one random
property tile via the seeded RNG and puts it in the spotlight -
`rules.spotlight_rent_pct`/`spotlight_duration_turns` (100%/8 turns in
the base mod, 0 = off) - composing multiplicatively with the boost and
forecast above; state lives on `GameState`, not `TileState`, so it
survives a trade/expropriation/bankruptcy of the spotlit tile untouched
(unlike the ADR-0012 boost, which resets on transfer); re-landing
re-rolls and replaces whichever tile was spotlit.

Deliberate simplifications (documented, do not "fix" without discussion):
no immediate interest when mortgaged tiles change hands; jail cards are a
count (not tradeable, never leave the deck rotation); per-game settings are
rules + timers only - the board/mod set is still chosen at room creation, not
lobby-editable (ADR-0015).

## Code conventions

- Rust 2021, rustfmt defaults. Comments document intent, invariants, and
  non-obvious behavior only - never restate code. Plain ASCII in code,
  filenames, and game data (tile names included).
- High cohesion, low coupling, explicit boundaries; composition over
  inheritance; no speculative abstraction. New variability goes behind the
  existing seams (Strategy traits, `ModPlugin`, `IdentityVerifier`,
  `GameHistory`) - do not invent new ones without need.
- Errors: typed enums (`CommandError` serializes with tag "code" and is
  sent only to the offending player as `Rejected`). Add a variant rather
  than overloading an existing one with a wrong message.
- Tests: every non-trivial engine rule gets a test in
  `crates/engine/tests/engine.rs` (use `FixedDice` and the `plain_board`/
  `transit_board` fixtures). Keep the wire-format tests green - they are
  compatibility contracts.
- Small changes as focused diffs; do not reformat unrelated code.
- Known borrow pitfall: `Exec::owned_property` returns
  `&'e PropertyDef` borrowed from `content` (not `&self`) precisely so
  callers can mutate state afterwards - follow that pattern for similar
  helpers.

## Untested / rough surfaces (be careful, verify before relying)

- `clippy --all-targets -- -D warnings` and `cargo fmt` now pass locally
  (first run reformatted the tree and fixed one `unit_arg` lint). Keep them
  green; fix warnings rather than silencing them.
- `Dockerfile` builds and the image serves `/healthz`, `/`, and static
  assets correctly (verified locally end-to-end, including the `webbuild`
  stage and the `--web-dir`/fail-loud boot check - Docker 28). Multi-stage:
  a self-contained Flutter Web build stage (manual checksummed SDK
  install, not a third-party image) feeds the final debian:bookworm-slim
  stage alongside rust:1.96-slim -> bookworm-slim for the server.
- The web OIDC flow (`oidc_login_web.dart`, popup + `postMessage`) has
  never been exercised against a real browser + real identity provider -
  only `flutter build web` compilation was verified. Popup-blocker
  behavior varies by browser (Safari is the strictest); treat this as a
  required manual QA step before relying on it, not CI-covered. The
  native flow (`oidc_login_io.dart`) is unchanged and still covered by
  `test/oidc_test.dart`.

## Roadmap (agreed next steps, roughly in order of value)

V2 ruleset DONE (ADRs 0017-0024, accepted and built 2026-07): the
six-step build order in `docs/business-tour-direction.md`, "V2 ruleset"
section, is complete - `mods/classic` was removed at step 6.

1. Flutter client polish (`clients/flutter` exists: full protocol, board,
   trades, tests; still needs real multiplayer playtesting, FX + audio -
   owner priority 2026-07 - and Android/mobile targets, postponed by
   owner). The visual identity is specified in `docs/visual-identity.md`
   (Art Deco geometry, validated palette, FR+EN via gen-l10n, ranked
   menu greyed until a matchmaking service exists).
2. Identity: verifier DONE (`eddsa.rs`, ADR-0009); OIDC login flow DONE in
   the Flutter client (`oidc.dart`: PKCE + system browser + loopback; web
   and CLI paste the token manually). Remaining: run the deploy on the
   personal server (`compose-deploy.yml` + `docs/deployment.md`; Rauthy
   client id `parcello`, EdDSA id tokens, loopback-wildcard redirect).
   HS256 removal is DEFERRED until LAN/WAN playtests have happened (owner
   decision, 2026-07) - do not delete it before then.
3. WASM mods: Wasmtime-backed `ModPlugin` implementation (V2 of the mod
   layer; the trait is already the seam). Unblocked since the MSRV moved
   to 1.96; pick a current Wasmtime.
4. Richer history queries (SQLx behind `GameHistory` if dashboards ever
   need it - see ADR-0005 first).

When picking up any item: state assumptions briefly, write the ADR if it
deviates from `docs/architecture.typ`, add tests, keep `--locked` green on
1.96, and update README + this file.
