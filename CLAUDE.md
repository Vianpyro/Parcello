# CLAUDE.md

Parcello: open-source multiplayer board game (Business Tour / Monopoly
style). Authoritative Rust server, thin clients, community-hosted servers
(Minecraft model), data-driven TOML mods. This repo is the complete,
playable backend V1: pure engine, mod layer, WebSocket server with an
embedded browser client, terminal test client, SQLite history.

Authoritative documents, in order of precedence:
1. `docs/architecture.typ` - the design document (game vision, layer rules,
   required patterns). Any deviation from it REQUIRES a new ADR in
   `docs/adr/` (short: context / decision / consequences).
2. `docs/adr/0001..0005` - accepted deviations. Read them before touching
   the engine, auth, mods, or history. Do not silently contradict them.
3. `README.md` - user-facing behavior reference (rules implemented, flags,
   protocol summary, known limitations).

## Hard constraints (do not break)

- **MSRV is Rust 1.75** (`rust-version` in the workspace). `Cargo.lock`
  carries load-bearing pins for transitive deps that resolve past 1.75:
  clap `<4.6`, indexmap `2.7.1`, tempfile `<3.15`, rusqlite `<0.32`, and
  tokio-tungstenite `0.24` deliberately aligned with axum 0.7 (this keeps
  the whole url/idna/icu family OUT of the graph - do not reintroduce it).
  Consequences:
  - Always build/test with `--locked`. Never run bare `cargo update`.
  - Adding a dependency: pick a version compatible with 1.75, then verify
    `cargo build --workspace --locked` still passes; pin transitives with
    `cargo update -p <crate> --precise <ver>` if needed.
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
  (dice/cards would become predictable). Cash and trade offers are public
  by design.
- **Auction solvency invariant**: cash cannot change while
  `TurnPhase::Auction` is active. Bids are validated against cash at bid
  time, so the winner can always pay at settlement. This is WHY all four
  trade commands are rejected during auctions - keep it that way, and keep
  the invariant if you add any new cash-moving command.
- **Even-build/even-sell** applies everywhere houses move: `Build`,
  `SellHouse`, AND `StandardLiquidation`. If you touch one, keep the three
  consistent.

## Commands

```sh
cargo build --workspace --locked
cargo test  --workspace --locked          # 49 tests, all must pass
cargo run -p parcello-server -- --insecure-guest [--history game.db]
# Browser client: http://localhost:7878/   (create/join by 5-letter code)
cargo run -p parcello-cli -- --name alice --create
cargo run -p parcello-cli -- --name bob --join ABCDE
```

CI (`.github/workflows/ci.yml`): stable job runs `fmt --check`,
`clippy --all-targets --locked -- -D warnings`, tests; msrv job builds on
1.75 with `--locked`.

## Architecture map

Workspace crates and their single responsibility (strict layering,
architecture doc section 5; dependencies point downward only):

- `crates/engine` - pure synchronous rules. `lib.rs` wires strategies
  (`DicePolicy`, `RentCalculator`, `BankruptcyResolver` as `Box<dyn>`);
  `apply.rs` is the whole command pipeline (validate -> mutate clone ->
  emit events); `state.rs` (GameState, TurnPhase incl. `Auction` variant,
  TradeOffer), `content.rs` (GameContent, RentModel), `view.rs`.
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
  machine Lobby -> Active -> Finished; host = seat 0; 2..=6 players; rejoin
  by identity, last connection wins; rooms with zero connected seats
  dissolve after `IDLE_TIMEOUT` = 30 min; optional per-turn AFK timer
  `--turn-timeout <secs>` auto-plays the canonical action
  Roll/Decline/Pass/EndTurn for the acting seat, 0 = off default; any
  accepted command resets the clock), `auth.rs`
  (`IdentityVerifier` trait: insecure guests and/or HS256 JWT via
  hmac/sha2 - no `ring`; the real Identity Service later slots in behind
  the same trait, ADR-0003), `history.rs` (`GameHistory` port; in-memory
  adapter + `SqliteHistory`: dedicated writer thread owns the rusqlite
  connection, trait methods enqueue and never block, `Drop` drains -
  ADR-0005), `web/index.html` (embedded via `include_str!` - the server
  binary is the whole deployment).
- `crates/cli` - terminal test harness; keep it in sync with new commands
  (it is the cheapest end-to-end protocol check).
- `clients/flutter` - Flutter client (Windows desktop first; Dart, not part
  of the cargo workspace). Mirrors the web client feature-for-feature; see
  its README. Requires the Flutter SDK (`flutter analyze && flutter test`).
  When adding an Event or CommandKind, update it too (protocol.dart +
  main.dart), same drill as `web/index.html` and the CLI.

Mod set is resolved once per server at boot (ADR-0004), not per room.

## Game rules snapshot (what exists)

Movement + Go salary; buy/decline; **auctions on decline** (round-robin
left of decliner, strict raises, high bidder skipped until outbid, no bids
= unsold; `rules.auction_on_decline = 0` disables); rent models per tile
(`houses` default with full-group x2 unimproved - a singleton group counts
as full; `group_scaled` stations; `dice_scaled` utilities - scaled models
reject Build); build/sell with even rule; mortgage (price/2 out, +10%
floored to redeem, house-free group required, mortgaged tiles pay nothing
but count for ownership); taxes; cyclic seeded decks, card chains capped
at depth 4; jail (doubles escape without bonus roll, fine, forced fine on
3rd fail); get-out-of-jail-free cards (per-player count `jail_cards`,
`UseJailCard` before rolling, auto-spent instead of the forced 3rd-roll
fine; cards stay in the cyclic deck once drawn - a count, not tradeable
objects); doubles re-roll, 3 doubles -> jail; partial-payment bankruptcy
with even-aware liquidation (houses then auto-mortgages) and transfer to
creditor (mortgages carry as-is; bank refurbishes); **trading**
(asynchronous offers, any solvent player any time EXCEPT during auctions;
exempt from turn check like Resign; re-validated at acceptance - stale
offers reject without mutation; purged on bankruptcy; max 4 open per
proposer; offers are public); resign; last-player-standing win.

Deliberate simplifications (documented, do not "fix" without discussion):
no immediate interest when mortgaged tiles change hands; jail cards are a
count (not tradeable, never leave the deck rotation); the AFK timer is
opt-in (`--turn-timeout`, off by default).

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
- `Dockerfile` has never been built (no Docker in the sandbox). Multi-stage
  rust:1.75-slim -> bookworm-slim; verify before publishing images.
- `crates/server/web/index.html` has never rendered in a real browser.
  Protocol coverage was verified mechanically (all 32 Event variants and
  all 17 CommandKind tags match the enums), so remaining risk is
  layout/UX. When adding an Event or CommandKind, update this file AND the
  CLI, and re-check field names against the enums.

## Roadmap (agreed next steps, roughly in order of value)

1. Flutter client polish (`clients/flutter` exists: full protocol, board,
   trades, tests; still needs real multiplayer playtesting and
   Android/mobile targets).
2. Global Identity Service: asymmetric JWT + JWKS fetch as a new
   `IdentityVerifier`; deprecate the HS256 stopgap (ADR-0003).
3. WASM mods: Wasmtime-backed `ModPlugin` implementation (V2 of the mod
   layer; the trait is already the seam). Check MSRV impact first.
4. Per-room mod sets (reintroduces the room `Starting` state; ADR-0004
   documents what collapses today).
5. Private trade offers (requires per-player `ClientView`s - today the
   view is identical for all seats; this is a real protocol change).
6. Reconnect tokens; richer history queries (SQLx behind `GameHistory` if
   dashboards ever need it - see ADR-0005 first).

When picking up any item: state assumptions briefly, write the ADR if it
deviates from `docs/architecture.typ`, add tests, keep `--locked` green on
1.75, and update README + this file.
