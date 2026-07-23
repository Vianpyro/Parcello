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
2. `docs/adr/0001..0036` - accepted deviations. Read them before touching
   the engine, auth, mods, or history. Do not silently contradict them.
   0017-0024 are the v2 ruleset (implemented); 0026 the spotlight; 0028
   the animation-ack watermark (server timers wait for client rendering);
   0030 the client animation budget + motion profiles (paired with
   `docs/motion-language.md`, the game-feel reference doc); 0031 a
   bankruptcy releases the estate to the bank (nobody inherits); 0032 the
   server serves runtime client config at `GET /config.json` (operator-set
   defaults like the sign-in issuer, no bundle rebuild); 0033 a
   token-authenticated player picks a public in-game handle (`display_name` on
   `AuthPayload`, identity still the token `sub`, re-sanitized server-side);
   0034 ranked matchmaking with a PER-SERVER ladder (`--ranked`): Weng-Lin
   ratings (`skillratings` crate, chosen over patented TrueSkill and
   2-player Glicko-2) keyed to the token `sub` - never the handle, never
   guests - behind a new `RatingStore` port (read-modify-write, so NOT
   `GameHistory`), a widening-window queue on the existing WebSocket,
   matchmaker-created rooms with no host powers that auto-start and
   broadcast `ratings_updated`; a global cross-server ladder stays deferred
   (it needs signed results, ADR-0009's stats note); 0035 spectators + the
   bots showcase: `ClientView::for_spectator` (seatless - NO trade offers,
   ALL pending bids/votes masked), `spectate {code?}` picks the fullest
   human game else the showcase, spectators are not seats (never gate
   timers, commands refused at the transport, cap 32/room, but they DO
   keep a room from idling out), and `--showcase` runs a supervisor that
   keeps one all-bot self-replaying room alive only while no humans play;
   0036 (Accepted, temporary) the gameplay WebSocket opens at connect and is
   held across the menu, kept warm by the server's ~25s native Ping/Pong
   heartbeat (`ws.rs` writer task, transport-only, no protocol change); the
   lazy room-scoped socket is the documented fallback, to revisit when ranked
   is wired client-side or idle-lobby connections become real pressure; 0037
   the token lifecycle + transparent session recovery: the client requests
   `offline_access`, keeps the whole grant (`OidcTokens`), and `AuthManager`
   (`auth_manager.dart`) renews the id_token ~120s before `exp` (timer) AND
   lazily before every auth payload (timers don't fire while a laptop
   sleeps), single-flight, honouring refresh-token rotation - the refresh
   token is MEMORY-ONLY, never persisted; `GameSession` reconnects a dropped
   socket with backoff and re-sends `join` automatically so the seat is
   reclaimed with no user action; server-side, `exp` gains a 60s
   `CLOCK_SKEW_LEEWAY_SECS` via the shared `auth::is_live`.
3. `README.md` - user-facing behavior reference (rules implemented, flags,
   protocol summary, known limitations).

Companion documentation (derived from the three above - they never
override them; added 2026-07 as the maintainer handbook):
- `docs/INVARIANTS.md` - the canonical must-always/must-never catalogue
  with per-entry enforcement locations. Audit any plan against it FIRST.
- `docs/AI_ENGINEERING.md` - how to work here: reading order, decision
  process, ADR how-to, review methodology, repo-specific pitfalls.
- `docs/extension-guides.md` - step-by-step recipes (new command, event,
  rule, message, timer, flag, mod, port...) with review checklists.
- `docs/domain-model.md` - every game concept -> type, invariants,
  lifecycle, owning ADR; plus the deliberate-simplification list.
- `docs/security-model.md`, `docs/testing.md`, `docs/performance.md` -
  threat model, test philosophy/map, and the anti-optimization list.
- `docs/technical-debt.md` - the known-debt register (delete entries as
  they are repaid); `docs/roadmap-and-product.md` - phased product path.
- `docs/LLM_CONTEXT/` - self-contained per-subsystem summaries for
  partial-context readers; update in the same change as their sources.
- `docs/LEGACY.md` - the project's spirit, lessons, and open questions.
- `DESIGN/` - the Design Bible: the visual/UX equivalent of the above,
  added 2026-07. It sits UNDER architecture and the two canonical design
  docs (`docs/visual-identity.md` = palette/fonts/board spec,
  `docs/motion-language.md` = motion doctrine + event catalogue, binding
  via ADR-0030); DESIGN/ builds on them, never restates their tables.
  Start at `DESIGN/README.md`; changes to visual rules use the DDR
  process (`DESIGN/DESIGN_DECISION_RECORDS.md`), review via
  `DESIGN/DESIGN_REVIEW.md`. Design adapts to the architecture, never the
  reverse.

## Hard constraints (do not break)

- **MSRV is Rust 1.96** (`rust-version` in the workspace; tracks recent
  stable since the 2026-07 dependency refresh - the old 1.75 pins are
  gone). Keep the CI msrv job, the Dockerfile base image, and
  `rust-version` in step when bumping. Consequences:
  - Build/test with `--locked` (reproducibility); `cargo update` is
    allowed but run `cargo test --workspace` and `cargo audit` after.
  - Dependency hygiene is CI-enforced by `cargo deny check` (deny.toml:
    RustSec advisories, permissive-only license allowlist - commercial
    distribution is planned - duplicate visibility, crates.io-only
    sources) plus `cargo machete`; `cargo audit` remains fine as a local
    habit but the gate is deny.
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
  parties (ADR-0007) - the server sends `ClientView::for_seat` views (or
  `for_spectator`, which hides ALL offers and masks every pending
  bid/vote, ADR-0035), never the omniscient `of`.
- **Auction solvency invariant**: cash cannot change while
  `TurnPhase::BlindAuction` is active. Bids are validated against cash at bid
  time, so the winner can always pay at settlement. This is WHY all four
  trade commands are rejected during auctions - keep it that way, and keep
  the invariant if you add any new cash-moving command.
- **Even-build/even-sell** applies everywhere houses move: `Build`,
  `SellHouse`, AND `StandardLiquidation`. If you touch one, keep the three
  consistent.
- **Lint baseline** (2026-07): `[workspace.lints]` in the root Cargo.toml
  enforces clippy `pedantic` + `nursery` (CI's `-D warnings` makes them
  hard) and `unsafe_code = "forbid"`. The short allow-list there is the
  only sanctioned escape hatch - justify new entries in that file, don't
  sprinkle `#[allow]` in code (site-level allows are reserved for
  data-shaped literals/translation tables, each with a reason).

## Commands

```sh
cargo build --workspace --locked
cargo test  --workspace --locked          # 200 tests, all must pass
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

CI (`.github/workflows/ci.yml`, redesigned 2026-07): five parallel jobs
- lint (fmt + typos + clippy `-D warnings` over the workspace pedantic
baseline; typos config in `_typos.toml`),
test+coverage (one instrumented `cargo llvm-cov` run; line coverage must
stay >= `COVERAGE_MIN_LINES` = 88% (ratchet: keep 2-3 pts under measured) after the documented exclusions -
cli/, server main.rs, lan.rs - and lcov+HTML upload as artifacts), msrv
(1.96 `cargo check --all-targets --locked`), rustdoc (`-D warnings`),
deps (`cargo machete` + `cargo deny check`, see deny.toml) - all
aggregated by
the single `CI OK` job (the only check branch protection needs). Docs-only
paths skip CI; PR pushes cancel outdated runs; a weekly cron re-runs on
main as a bit-rot/advisory canary. `flutter.yml` gates the client
(analyze + test + web build) only when `clients/flutter/**` changes;
`web-perf.yml` is the client's perf gate on the same paths (pinned Flutter
3.44.6): a deterministic Brotli size budget (`clients/flutter/tool/size_budget.sh`
+ `perf-budgets.json`) plus Lighthouse CI on the served build
(`lighthouserc.json`, desktop preset), reports uploaded as artifacts, both
fail the run on a breached budget - `docs/web-performance.md` explains reading
and tuning them (note: `flutter build web --analyze-size` is AOT-only, so the
size analysis is `--source-maps` + `tool/analyze_size.py`); `codeql.yml`
handles security static analysis. No cargo features and no
benches exist in the workspace - revisit the matrix/bench story if either
appears.

Releases (`.github/workflows/release.yml`): bumping the workspace version
in `Cargo.toml` on main tags `vX.Y.Z` and publishes a GitHub release with
server+CLI binaries (linux x64/arm64, windows, macos arm64; `mods/`
bundled), Flutter client bundles (windows/linux/macos), all-in-one
archives for windows and linux (client + server, Steam-depot-shaped; the
linux one also fits the Steam Deck), and a GHCR image
(`ghcr.io/<owner>/parcello-server`, amd64). `Cargo.toml`'s workspace
`version` is the SINGLE source of truth; the Flutter client stamps it by
injecting it at build time -
`--dart-define=PARCELLO_VERSION="$(clients/flutter/tool/cargo_version.sh)"` on
every `flutter build` (the same transport as the git SHA), read as a const in
`lib/version.dart`. `pubspec.yaml`'s `version` is a placeholder Flutter
requires - never hand-edit it, nothing reads it for display. The release goes
live only after all binary jobs succeed
(draft-then-publish); the docker job is independent. Binaries use the
size/perf `[profile.release]` (LTO, codegen-units=1, strip). A `checksums`
job runs between the binary jobs and publish: it asserts all nine expected
archives are present, validates each archive (readable + carries the server
binary), writes `SHA256SUMS`, and best-effort keyless-signs it with cosign
(Sigstore/GitHub OIDC, `id-token: write`) - signing never fails the release
(the slot is where Authenticode/notarization would later be added over the
same sums). `.github/release.yml` categorizes the auto-generated notes by PR
label; `.github/dependabot.yml` keeps the pinned actions current. Dependency
licenses are all permissive (checked 2026-07 with cargo-license; keep it
that way - commercial distribution is planned).

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
  `apply.rs` is the command pipeline entry (validate -> mutate clone ->
  emit events), with the `Exec` methods split by domain under `apply/`
  (movement, jail, trade, auction, estate, landing, cash, turn - all
  `pub(super)`, the pipeline stays the only entry point); `tuning.rs`
  gathers the fixed game-policy numbers that are NOT mod-configurable
  (VP weights, mortgage/refund percents, the 10% discoverer rebate
  - promote to `RuleParams` with an ADR before making one moddable);
  `state.rs` (GameState, TurnPhase incl. `BlindAuction` and
  `BribeVote` variants, TradeOffer; the public market layer - forecast +
  spotlight types - lives in `state/market.rs`, re-exported so paths
  never changed), `content.rs` (GameContent, RentModel; `group_tiles` is
  a lazy iterator - rent/VP checks walk groups on every landing, no Vec),
  `view.rs`. `bot.rs`
  is the shared autopilot heuristic (`bot::decide(content, view, seat,
  noise) -> Option<CommandKind>`): pure like everything here, used by both
  the server's bot seats and the CLI `--bot` (ADR-0014); its unit tests
  live in `bot/tests.rs` (same split-for-size pattern as `room/tests.rs`).
- `crates/mods` - TOML mod bundles. `RegistryBuilder` merges
  last-loaded-wins per key (tiles/cards replace in place by id, rule
  scalars override; conflicts logged WARN). Base game content is itself a
  mod (`mods/base/`). `resolve()` -> `ResolvedContent` (pushed verbatim to
  joining clients: mod distribution MVP).
- `crates/protocol` - JSON envelopes (`ClientMessage`/`ServerMessage`).
  Commands/events on the wire ARE the engine's serde types (externally
  tagged, snake_case) - the wire format is the replay format. Wire-format
  tests exist; changing serde shapes is a protocol break.
- `crates/server` - axum, split lib + thin binary (2026-07): `lib.rs`
  exposes the modules, `AppState`, and `game_router` so the WebSocket
  integration tests in `tests/ws.rs` boot the real router on an ephemeral
  port; `main.rs` only parses flags and wires (`build_state`, anyhow at
  the app boundary). `ws.rs` (transport: parse, authenticate once at
  create/join, relay - the room-scoped messages funnel through one
  exhaustive `relay` fn, so a new `ClientMessage` variant fails
  compilation there; inbound frame + message size are capped at
  `MAX_WS_MESSAGE_BYTES` = 64 KiB so an untrusted client cannot force a
  large allocation before a message is parsed/validated - a *read* limit,
  so it never truncates the larger server -> client snapshots; a global
  `AppState.connections` semaphore caps concurrent sockets at
  `MAX_CONNECTIONS` = 1024 and a per-connection token-bucket `RateLimiter`
  (`MSG_BURST` = 32, `MSG_REFILL_PER_SEC` = 16) closes a flooding client -
  per-IP throttling is delegated to the deployment's reverse proxy), `room.rs` (one Tokio task per room; state
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
  canonical action (a bot-chosen movement card via `bot::movement_card` -
  smart, not just the lowest - / ascending Legal Route / EndTurn,
  ADR-0017/0024; movement/route/end only, an AFK auto-play never spends
  the seat's cash - `BlindAuction`/`BribeVote` have no single actor, so
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
  per-turn countdown the clients reset once their animations finish;
  animation-ack watermark (ADR-0028): every `Update` carries a monotonic
  `seq`, clients ack rendered-through-N via `animation_done`, and the
  bid/vote windows (table-wide), turn clock/bank drain (acting seat) and
  `BOT_THINK` (table) wait for the watermark, bounded by
  `ANIM_ACK_CAP` = 10s - bots/disconnected seats/the CLI settle instantly,
  the game clock is never gated; post-game
  survey `feedback` message:
  Finished phase only, once per seat, rating 1-5 + comment run through
  `sanitize_comment` (strip control chars + Unicode bidi/zero-width format
  chars, cap `COMMENT_MAX_CHARS` = 500 scalar values) before it is stored
  via `GameHistory::record_feedback` (parameterized SQL, no injection) -
  the untrusted comment can never carry a terminal-escape/log-injection or
  display-spoofing payload; the client UI must
  stay non-blocking, side card not modal; the room's clocks live in
  `room/clock.rs` and the play-for-absent-humans logic - AFK canonical
  action, silent-bid/vote injection, bot turns - in `room/autoplay.rs`),
  `auth.rs` + `eddsa.rs`
  (`IdentityVerifier` trait: insecure guests, EdDSA identity tokens
  verified against JWKS from any OIDC provider - Rauthy is the reference,
  ADR-0009 - and the deprecated HS256 stopgap, ADR-0003; tokens dispatch
  on the header `alg`), `history.rs` (`GameHistory` port; in-memory
  adapter + `SqliteHistory`: dedicated writer thread owns the rusqlite
  connection, trait methods enqueue and never block, `Drop` drains -
  ADR-0005), `ranked/` (ADR-0034: `ladder.rs` pure Weng-Lin math +
  placement derivation, `store.rs` the `RatingStore` port with
  memory/SQLite adapters, `queue.rs` the widening-window pool + matchmaker
  task), `showcase.rs` (ADR-0035: `--showcase` supervisor task keeping one
  all-bot self-replaying room alive only while no room has an Active game
  with a connected human; rooms answer `RoomCmd::Probe` for it and for the
  spectate picker; spectators themselves live on the room -
  `RoomCmd::SpectateJoin`, `ClientView::for_spectator` views, trade events
  filtered, acks ignored, cap `MAX_SPECTATORS` = 32, and a watched room
  never idles out), `lan.rs` (opt-in `--lan` UDP discovery announcer:
  periodic
  multicast to `239.255.0.1:55888` with optional broadcast fallback so LAN
  clients find the server without a URL; best-effort, detached, no admin
  control plane - local process management is the client's job, ADR-0016),
  the Flutter Web client (served from disk at runtime via `tower-http`'s
  `ServeDir`, `--web-dir`/`PARCELLO_WEB_DIR`, default `web` - mirrors
  `--mods-dir`'s pattern, not compiled into the binary; fails loudly at
  boot if the directory has no `index.html`, same idiom as
  `parcello_mods::resolve`'s `?` propagation - ADR-0025).
- `crates/cli` - terminal test harness; keep it in sync with new commands
  (it is the cheapest end-to-end protocol check; `addbot`/`rmbot`,
  `set <field> <value>`, and `mods` (prints the server's `list_mods`
  answer) stdin commands too, ADR-0015; `--spectate [CODE]` watches a game
  seatless, bare form lets the server pick (ADR-0035); the `discover` bin
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
  loopback redirect on desktop vs. popup + `postMessage` on web; both
  return the full `OidcTokens` grant that `AuthManager` then renews,
  ADR-0037),
  `lan_discovery.dart` and `server_manager.dart` (native-only, no browser
  equivalent - stubbed out on web, hidden behind `kIsWeb` in the menu),
  `session_storage.dart` (a file on desktop, `localStorage` on web). When
  adding an Event or CommandKind, update it too (protocol.dart +
  main.dart), same drill as the CLI.

  **Localization** (gen-l10n, `flutter: generate: true`): every user-facing
  string is an ARB key in `lib/l10n/app_en.arb` (template) + `app_fr.arb`,
  surfaced as `AppLocalizations.of(context)` - never a literal in a widget.
  The generated `lib/l10n/app_localizations*.dart` is gitignored (rebuilt by
  `flutter pub get`; CI runs it explicitly, flutter.yml). Config in
  `l10n.yaml`. The main menu is a card grid: a `_PrivateTableCard` whose
  split footer holds Create (one tap, server-default mods) / Modded / Join -
  the latter two expand *inline* in the card, never a modal; the mod picker
  is fed by `list_mods` (tap-to-order chips, same pattern as the Legal Route
  builder, order matters per ADR-0006; empty selection sends no `mods`
  field) - plus `_MenuTile`s (including "Watch a game" - bare `spectate`,
  ADR-0035: `GameSession.spectating`, seat null, action-free game screen
  with a side-panel badge, `test/spectate_and_hints_test.dart`) and a static
  `RulesScreen`. **Onboarding**: first-game coach marks
  (`ui/coach_mark.dart` + the hint state on `GameSession`): ONE contextual,
  never-modal hint at a time (lobby / hand / sealed bid / jail / VP race),
  shown the first time its situation comes up, dismissed forever (persisted
  under the `_hints` reserved key beside the reconnect tokens), re-armed by
  the menu's "replay tips" button; spectators get none. The connect screen
  probes the *typed* server's `/config.json` (debounced; ADR-0032 +
  `guest_allowed`): a definitive `guest_allowed: false` disables the guest
  path (sign-in becomes primary, Connect requires a token), and the probe
  doubles as a liveness indicator (web cross-origin failures stay
  "unknown", never "unreachable" - could be CORS).
  The three OFL fonts (Inter/Fraunces/SourceSerif4) are
  bundled offline under `assets/fonts/` (SHA256SUMS + README there), Inter
  wired as the theme family and their licences registered via
  `LicenseRegistry`. The event log is localized too: `describeEvent`
  (protocol.dart) takes an `AppLocalizations`, and `GameSession` (a
  context-free `ChangeNotifier`) is handed one each frame by `ParcelloApp`'s
  builder so log lines localize before any server message is processed.

  **Layout headroom** (`test/layout_test.dart`): the game screen is rendered at
  the sizes we ship to - Steam Deck 1280x800, the 1280x720 default window, and
  a measured 1024x600 floor - with a *loaded* room (six open trades, six seats,
  a running clock), because an overflow is a layout error in a pumped frame.
  The side panel scrolls for exactly this reason: it grows with the room, and
  six offers overflowed it by 527px on a Deck - resolution had nothing to do
  with it. Below 1024x600 the board's centre cannot hold the HUD; that is the
  floor, not a bug to chase.

  **Controller / Steam Deck**: keyboard-focus navigation, which Steam Input
  maps onto a gamepad. Menu tiles (`_MenuTile`) and board tiles carry a
  visible gold focus ring; board tiles are focusable ONLY when actionable
  (`canAct`), via `FocusableActionDetector` (D-pad traverses, A =
  `ActivateIntent` opens the tile menu). `FocusTraversalGroup`s scope the menu
  grid and the `_Actions` buttons; `RulesScreen` pops on Escape (controller
  B). No autofocus on frequently-rebuilt panels (it would steal focus).

  **Motion / game feel** is specified in `docs/motion-language.md` (the
  reference doc: philosophy, tiers, visual grammar, the full event
  catalogue, and an honest "built vs. not built" list) and ADR-0030 (the
  animation budget + motion profiles). Read both before touching anything
  animated. The client-side split, dependencies pointing downward:
  - `tokens.dart` - the palette and geometry from
    `docs/visual-identity.md`. A colour exists here or nowhere; a hex
    literal at a use site is a bug.
  - `motion.dart` - every duration, curve, tier and the animation budget.
    A duration literal anywhere else is a bug: the director and the pawn
    layer used to derive their timing independently and could drift.
  - `stage.dart` - `StageState`, the *transient* visual state (what the
    board is showing), deliberately a SEPARATE notifier from
    `GameSession` (what the server says is true) so animation frames
    never repaint the action panel's text fields.
  - `director.dart` - `compile(events, ctx) -> Plan` is **pure** (no
    socket, no widgets, no clock) and is where tiers, lanes, coalescing
    and the budget are decided; `session.dart` only executes the plan and
    acks. When adding an Event with a visual, give it a beat in
    `_beatsFor` - and add a test, because the budget invariant is
    checkable (`test/director_test.dart`).
  - `overlay.dart` - what crosses between board and HUD (money chits) and
    the P1 arrest.
  - ADR-0028's contract is unchanged: Updates queue, play in order, the
    view applies after the beats, the `animation_done` ack releases the
    server's gated timers. ADR-0030 adds that the client must never
    exceed its animation budget (tiered 8s/6s/4s by the loudest beat in
    the Update) against the server's `ANIM_ACK_CAP` = 10s -
    a client that outruns the cap is not slow, it is *behind the game*.

Mods: the server resolves a default set at boot (`--mod`), and each room
may override it at creation via the optional `mods` field on Create
(ADR-0006; ids are allowlist-validated in `ws.rs` because they become
filesystem paths). Default `mods/base` is the 32-tile fast board (9x9
ring, no Community Chest, two "utility" tiles (Wi-Fi, The Chatbot -
group-scaled, modern reinterpretations of the original resort idea), two
chance tiles, one net-worth tax tile (The Audit, ADR-0029),
`docs/business-tour-direction.md`);
`mods/highroller` is a rules-only example. Clients render any `4*(d-1)`
square ring (32, 40, ...); other tile counts fall back to a wrap layout.

## Game rules snapshot (what exists)

Movement is a velocity deck, no dice (ADR-0017): `PlayMovementCard` plays
a value from a public `Player.hand`, refilled to
`rules.velocity_min..=velocity_max` the instant it empties - that refill
also ticks `Player.hands_cycled`, ADR-0020's round metronome; plus Go
salary. The starting player is seed-drawn (2026-07), not the host.
Sealed-bid auctions on every landing (ADR-0018): a 12s
`TurnPhase::BlindAuction` window (raised from 5s after playtests), every
living seat bids at once via
`SubmitBlindBid` (0 abstains), the market-price floor binds EVERY non-zero
bid (ADR-0018 amended 2026-07: the old discoverer-only floor let a rival
buy for 1$ whenever the discoverer was too broke to hold its implicit
bid - a seat that can't afford the price now abstains, full stop), the
discoverer gets an implicit list-price floor bid if silent and solvent,
ties favour the discoverer then
the lowest seat, an all-zero result leaves the tile unsold - no plain
decline any more. Every winner pays their bid IN FULL; a discoverer that
wins is then rebated `DISCOVERER_REFUND_PCT` = 10% of what it paid, as its
own `Event::DiscovererRefunded` (ADR-0018 amended 2026-07: the old "90% on
a contested win above the floor" was invisible - a discount never happens
on screen - and conditional in a way nobody could hold in their head; the
rebate is the discoverer's whole edge on price, and the two motions are
what the table watches). Do not fold it back into the settlement. Rent models per tile (`houses` default with full-group
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
(`ChooseLegalRoute`, a locked public permutation of the full FRESH hand -
every velocity value, `velocity_min..=velocity_max`; whatever was left in
hand is DISCARDED, so this is not a permutation of the cards you still
hold, and a client that offers those builds a command the engine can only
reject - the
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
with even-aware liquidation (houses then auto-mortgages); the estate is
then RELEASED TO THE BANK, never inherited (ADR-0031, 2026-07): every tile
goes back unowned/unmortgaged/stripped and must be won again through the
normal sealed-bid auction, and the creditor gets only the residual cash -
`Event::PlayerBankrupt { creditor }` now means "who took the cash". Same
path as `Resign`. Do not reintroduce inheritance: it was the biggest
luck-driven snowball in the game (the creditor is whoever happened to own
the tile you landed on, and a portfolio carries its ADR-0020 victory
points with it); **trading**
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
has completed a hand refill, ties to the lowest seat - `Player.hands_cycled`
is public in `PlayerView` so clients can render round progress (the round
number is the MIN across survivors; the bonus fires when the last straggler
refills and lifts it), plus `Event::RoundBonusAwarded`; reaching the target
ends the game instantly, `Event::WonByPoints`; if a `Build` empties the
conglomerate pool first, the game ends immediately too - highest score
wins, ties by net worth then lowest seat, `Event::WonByPoolExhaustion`,
the "doom clock"). Optional expropriation (`rules.expropriation`, seize a
rival's property at a premium, landing tile only, end of turn; improved
tiles liquidate to the shared pools, owner compensated, ADR-0011/0022)
and rent boosts (`rules.rent_boost`, +50%/step, cap 3, reset on transfer,
ONE-SHOT since the 2026-07 amendment: the first rent collected at the
boosted rate consumes the whole boost, `Event::RentBoostConsumed`,
ADR-0012) - both on in the base fast mod. A rival's MORTGAGED tile is no
longer takeover-proof: landing on it lets you buy it at its flat mortgage
value (price/2 to the owner, transfers still mortgaged, ADR-0022 amended)
- the mortgage is now the cheap-buyout weak point, not the shield. Taxes:
the base mod's only tax tile is The Audit (`TileKind::NetWorthTax`,
ADR-0029) at the last tile before Go - a seeded-random 5-25% slice of the
lander's net worth, heavier brackets linearly rarer. Public market forecast
(ADR-0021, `data/events.toml`): a seeded rolling queue of the next 3
scheduled events plus whichever is active - `rent_multiplier`,
`acquisition_multiplier`, one-shot `wealth_tax` - `gap_turns` apart,
public in every view (draws already made, never the generator).
`acquisition_multiplier` moves the PRICE, not the settlement (ADR-0021
amended 2026-07): `Exec::market_price` is the ONE reference - auction floor,
the discoverer's implicit bid, `BidBelowFloor`, takeover cost, and the
client's `marketPrice` (protocol.dart) all read it. Settlement pays the bid
as-is; re-applying the multiplier there would compound (-20% settling at
-36%). The bot bids against it too, or it would overbid a crashed floor. The
Exposition corner (`TileKind::Spotlight`, ADR-0026, replaces the old
no-op `free_parking` in `mods/base`): landing there draws one random
property tile via the seeded RNG and puts it in the spotlight -
`rules.spotlight_rent_pct`/`spotlight_duration_turns` (100%/permanent in
the base mod; duration <= 0 = permanent-until-replaced since the 2026-07
amendment, pct 0 = off) - composing multiplicatively with the boost and
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
- Tests: every non-trivial engine rule gets an integration test under
  `crates/engine/tests/` - themed files (`engine.rs` core flow/wins,
  `auction_and_trade.rs`, `jail_and_corruption.rs`,
  `estate_and_economy.rs`) over the shared fixtures in
  `tests/common/mod.rs` (`plain_board`/`transit_board`/...). Keep the
  wire-format tests green - they are compatibility contracts.
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
   trades, tests; still needs real multiplayer playtesting and
   Android/mobile targets, postponed by owner). The visual identity is
   specified in `docs/visual-identity.md` (Art Deco geometry, validated
   palette, FR+EN via gen-l10n, ranked menu greyed until a matchmaking
   service exists); the motion language and game feel in
   `docs/motion-language.md` + ADR-0030. The palette and the motion
   architecture landed 2026-07. `DESIGN/IMPLEMENTATION_ROADMAP.md` is now
   the maintained, re-audited list of what is built vs not (it supersedes
   motion-language section 13's honesty list, which drifted - fonts, the
   discoverer rebate chit, and rent-to-the-earner are all built, contrary
   to older revisions). In rough priority order the remaining client gaps
   are: **anchoring the sealed-bid input to the lifted tile** (the tile
   already lifts/recedes; the gap is the INPUT and the window clock as a
   hairline draining on the tile's own edge, not a corner number - the
   biggest remaining spec/build gap), trade animations, the AFK auto-play
   marker (the server plays your turn and nothing tells you), the
   time-bank alarm / bot-thinking pulse / reconnect re-orientation, then
   the audio pass (the clip set is placeholder; `assets/sfx/README.md`
   lists the four category earcons that are still silent). gen-l10n and
   the three OFL fonts are DONE.
2. Identity: verifier DONE (`eddsa.rs`, ADR-0009); OIDC login flow DONE in
   the Flutter client (`oidc.dart`: PKCE + system browser + loopback; web
   and CLI paste the token manually). Remaining: run the deploy on the
   personal server (`compose-deploy.yml` + `docs/deployment.md`; Rauthy
   client id `parcello`, EdDSA id tokens, loopback-wildcard redirect;
   Rauthy already grants refresh tokens and reissues an id_token on
   refresh, verified 2026-07, so renewal needs NO issuer config). Token
   renewal and transparent socket recovery are DONE client-side
   (ADR-0037) - that bug was entirely client-side.
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
