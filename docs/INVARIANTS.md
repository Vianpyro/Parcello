# Invariants

The canonical catalogue of things that must ALWAYS or NEVER happen in this
codebase. Scattered until 2026-07 across CLAUDE.md, ADR bodies, and code
comments; gathered here so a reviewer - human or AI - can audit a change
against one list.

**Authority**: this file *collects* invariants, it does not create them.
Each entry cites its source (an ADR or `docs/architecture.typ`). If this
file and an ADR ever disagree, the ADR wins and this file has a bug.
Every entry has four parts: the rule, why it exists, what breaks if it is
violated, and where it is enforced (a test, a type, or "review only" -
the last kind deserves the most reviewer attention).

Violating an invariant is never a refactor. It is an architecture change
and requires a new ADR *before* the code.

---

## E. Engine (`crates/engine`)

### E1. The engine is pure
No I/O, no async, no randomness source, no clock. Dependencies: `serde` +
`thiserror`, nothing else. (architecture.typ section 4; CLAUDE.md hard
constraints)

- **Why**: purity is what makes `(players, seed, commands)` a complete
  replay, makes every rule unit-testable without a runtime, and is a hard
  precondition for V2's WASM re-execution of rule overrides.
- **Breaks**: replays diverge between machines/versions; the
  `same_seed_produces_identical_games` guard and the fuzzer become
  meaningless; server-side cheat detection (re-run the log) becomes
  impossible.
- **Enforced**: review only, plus the dependency list in
  `crates/engine/Cargo.toml` (deny/machete flag additions). There is no
  automated purity test - treat any new `use std::time`, `rand`, or async
  in the engine as an automatic rejection.

### E2. Rejections never mutate (ADR-0001)
`Engine::apply` returns `Result<(GameState, Vec<Event>), CommandError>`;
on `Err` the input state is bit-identical to before. The accepted-command
log is therefore a deterministic replay.

- **Why**: the wire is the replay format; a partially-applied rejection
  would corrupt every stored game in `--history`.
- **Breaks**: history logs stop replaying; desync between the server's
  live state and what a rejoining client reconstructs.
- **Enforced**: `apply.rs` pipeline shape (validate, then mutate a clone),
  `same_seed_produces_identical_games` (crates/engine/tests/engine.rs),
  and the fuzzer (`game_state_fuzzer.rs`), which panics if a generated
  command is rejected and asserts state invariants after every step.
- **Duty**: adding a `TurnPhase` variant requires extending the canonical
  actions the same-seed guard derives from `TurnPhase`.

### E3. Randomness comes only from `GameState.rng` (ADR-0002)
One SplitMix64 stream, seeded at `new_game`, advanced only by accepted
commands.

- **Why**: same seed + same commands = same game, on any machine.
- **Breaks**: E2's replay property.
- **Enforced**: E1's dependency rule (there is nothing else to draw
  randomness from) + the same-seed guard.

### E4. `ClientView` never exposes `rng` or deck order
Nor anything derived from them ahead of time (upcoming chance/community
draws, future market events beyond the published forecast queue).
(CLAUDE.md hard constraints; ADR-0021 for the forecast's "draws already
made, never the generator" line)

- **Why**: card draws and market events would become predictable to a
  client that computes ahead.
- **Breaks**: every seeded surprise in the game.
- **Enforced**: `view.rs` field selection (there is no `rng` field on
  `ClientView`); review when adding view fields.

### E5. Per-seat privacy is masked in the view, not the client
Trade offers are visible only to their two parties (ADR-0007). While a
`BlindAuction` or `BribeVote` window is open, a seat sees only its OWN
pending bid/vote (ADR-0018/0024); everything is revealed in the
resolution event. A spectator sees NO offers and NO pending entries
(ADR-0035). The server sends `ClientView::for_seat` or `for_spectator`,
never the omniscient `of` (test/replay tooling only).

- **Why**: sealed bids are the entire point of ADR-0018; trades leak
  negotiating positions; clients are untrusted renderers, so masking
  client-side would be no masking at all.
- **Breaks**: sealed auctions become open auctions; trade metagame dies.
- **Enforced**: `seat_view_shows_only_own_trade_offers`,
  `spectator_view_masks_every_pending_bid`
  (crates/engine/tests/auction_and_trade.rs), the ws integration test
  `spectator_watches_without_a_seat_and_sees_nothing_private`, and
  `event_visible_to` / `event_public` in `room.rs` for the event feeds.

### E6. Auction solvency freeze (ADR-0018)
Cash cannot change while `TurnPhase::BlindAuction` is open. Bids are
validated against cash at submit time. This is WHY all four trade
commands are rejected during the window.

- **Why**: the winner must always be able to pay at settlement; no
  "won the auction, then paid a rent, now insolvent" states.
- **Breaks**: settlement can drive cash negative outside the bankruptcy
  path, corrupting the partial-payment machinery.
- **Enforced**: trade-rejection-during-auction tests
  (auction_and_trade.rs); extend them if you add ANY new cash-moving
  command.

### E7. The universal bid floor (ADR-0018, amended 2026-07)
Any non-zero sealed bid below the current market price is rejected
(`BidBelowFloor`) for EVERY seat; `0` is the only abstention. The
discoverer additionally holds an implicit floor bid when silent and
solvent.

- **Why**: the earlier discoverer-only floor let a rival buy a tile for
  1$ whenever the discoverer could not afford the implicit bid - playtest
  feedback read it as a glitch, not a snipe.
- **Breaks** (if re-loosened): the 1$ buy returns; (if tightened to
  forbid abstention): poor seats cannot legally act and the window never
  resolves.
- **Enforced**: `discoverer_pays_its_winning_bid_in_full_then_gets_the_rebate`
  (asserts the non-discoverer rejection too) and the fuzzer's bid
  generator (only `0` or `floor..=cash`).

### E8. `market_price` is the ONE price reference (ADR-0021 amended)
Auction floor, the discoverer's implicit bid, `BidBelowFloor`, takeover
cost, the bot's bidding, and the clients' displayed price all read the
same function. Settlement pays the recorded bid as-is - never re-apply
the market multiplier.

- **Why**: re-applying compounds (a -20% crash would settle at -36%).
- **Enforced**: `a_crash_moves_the_floor_itself_and_never_compounds`.

### E9. Even build / even sell, everywhere houses move
`Build`, `SellHouse`, AND forced `StandardLiquidation` follow the even
rule. If you touch one, keep all three consistent. (CLAUDE.md)

- **Enforced**: estate_and_economy.rs tests; review.

### E10. Bankruptcy releases the estate to the bank (ADR-0031)
Tiles return unowned, unmortgaged, stripped; the creditor takes only
residual cash. Never reintroduce inheritance.

- **Why**: inheritance was the game's biggest luck-driven snowball (the
  creditor is whoever you happened to land on, and a portfolio carries
  its victory points).
- **Enforced**: bankruptcy tests in estate_and_economy.rs.

---

## P. Protocol (`crates/protocol`)

### P1. The wire format IS the replay format
Commands and events on the wire are the engine's own serde types
(externally tagged, snake_case). Changing an existing serde shape is a
protocol break AND a replay break.

- **Why**: one format means a stored history row can be replayed and a
  client can be driven from the same JSON; two formats would drift.
- **Breaks**: every stored `--history` database, every deployed client.
- **Enforced**: the wire-format tests in `crates/protocol/src/lib.rs`
  and the engine's serde tests. They are compatibility contracts: a
  legitimate break must update them *and* say so in an ADR.

### P2. Protocol evolution is additive-only
New fields are `#[serde(default)]` + `skip_serializing_if`; new messages
are new externally-tagged variants. Old client + new server (and vice
versa) must keep working. Precedent: `mods` on Create (ADR-0006),
`display_name` (ADR-0033), `ranked` on Joined (ADR-0034), `spectate`
(ADR-0035).

- **Enforced**: the wire tests include "old client omits the field"
  cases; add one for every new optional field.

### P3. A new `ClientMessage` variant must fail compilation until routed
`ws.rs::relay` matches exhaustively; connection-scoped variants are
consumed earlier in `handle_socket` and listed in relay's unreachable
arm. Never add a `_ =>` catch-all there.

- **Why**: this is the compile-time guarantee that no message is
  silently dropped.

---

## S. Server (`crates/server`)

### S1. One Tokio task owns each room's state
All access goes through `RoomCmd` messages; nothing else holds `&mut`
Room. Replies use oneshots; broadcasts use per-connection unbounded
senders. There are no locks around game state.

- **Why**: the actor shape is what makes the room logic single-threaded
  and testable; every data race is designed out rather than locked out.
- **Breaks**: adding a shared `Mutex<GameState>` anywhere reintroduces
  the whole class of bugs this shape eliminates.
- **Enforced**: type structure (Room is not `Sync`-shared); review.

### S2. Identity is the verifier's `player_id`, never the display name
`guest:<name>`, `hs256:<sub>`, `id:<sub>` - seats, rejoin, history rows,
and ratings all key on it (ADR-0003/0009/0033). Display names are
mutable cosmetics, re-sanitized server-side. Ratings additionally refuse
spoofable (guest) identities outright (ADR-0034).

- **Breaks**: seat hijack by rename; rating theft; history corruption.
- **Enforced**: auth.rs tests (`token_display_name_overrides_but_never_leaks_or_spoofs`),
  ranked ws tests (guest rejection).

### S3. Untrusted wire values are validated at one named boundary each
- Room settings: `clamp_settings` + the `limits` module (room.rs).
- Mod ids: `valid_mod_id` (ws.rs) - they become FILESYSTEM PATHS.
- Broadcast text: `sanitize_comment`, `sanitize_display_name`,
  `sanitize_guest_name` (control chars + Unicode bidi/zero-width
  stripped; `@` handles rejected so emails never surface).
- Frame size: `MAX_WS_MESSAGE_BYTES` = 64 KiB (a READ limit - it must
  never constrain server->client snapshots).
- Message rate: `RateLimiter` (burst 32, refill 16/s) per connection.
- Concurrency: `MAX_CONNECTIONS` = 1024 semaphore; `MAX_SPECTATORS` = 32
  per room.

- **Why one named place each**: the trust boundary must be auditable at
  a glance. Do not scatter partial validation.
- **Enforced**: unit tests per validator (`mod_ids_cannot_escape_the_mods_dir`,
  `rate_limiter_allows_a_burst_then_blocks_then_refills`, sanitize tests).

### S4. The game clock is never animation-gated (ADR-0028)
The bid/vote windows, turn clock/time-bank drain, and bot pacing wait for
the animation-ack watermark, bounded by `ANIM_ACK_CAP` = 10s. The
absolute game deadline (ADR-0010) does not wait for anything.

- **Why**: a stalling (or malicious) client must never extend a game;
  but rendering time must never eat thinking time either.
- **Coupled contract**: the client's animation budget (ADR-0030: tiered
  8s/6s/4s + 2s slop) must stay under the 10s cap. Raising the budget
  without raising the cap reopens the desync ADR-0028 exists to prevent.
- **Enforced**: room clock tests (room/tests.rs), director budget test
  (clients/flutter/test/director_test.dart).

### S5. AFK auto-play never spends the seat's cash
The canonical action is a movement card / ascending Legal Route /
EndTurn only. Simultaneous windows (`BlindAuction`, `BribeVote`) have no
single actor: `acting_seat()` returns `None` for them and their own
timers auto-abstain (bid 0) / auto-reject silent seats.

- **Why**: the server must never gamble a player's money for them.
- **Enforced**: room/autoplay.rs + its tests.

### S6. Spectators are not seats (ADR-0035)
They never gate timers (acks ignored), hold no reconnect token, have
their game commands refused at the transport, and are addressed by a
unique `watch:<n>:<id>` routing key so their disconnects can never
shadow a seat's. They DO count against room dissolution (a watched game
stays alive) - and `RoomCmd::Probe` deliberately does NOT count as
activity, or the showcase supervisor's 15s probing would keep every room
alive forever.

- **Enforced**: ws tests (`spectator_watches_without_a_seat_and_sees_nothing_private`);
  the Probe/activity rule is review-only - it lives in one `matches!`
  guard in `Room::run` and is easy to break by "simplifying".

### S7. Persistence ports match their access pattern
`GameHistory` is append-only fire-and-forget: methods enqueue to a
dedicated writer thread and never block the room task (ADR-0005).
`RatingStore` is synchronous read-modify-write behind a mutexed
connection (ADR-0034) - reads happen off the async executor
(`spawn_blocking` in ws.rs); the single at-game-end write in the room
task is the documented, accepted exception. Do not "unify" the two
ports: their contracts are different on purpose.

### S8. Ranked rooms have no host, and rate exactly once
`Configure`/`AddBot`/`RemoveBot`/`Start`/`PlayAgain` are rejected;
placement is derived at `Finished` (winner first, survivors by VP then
net worth then lowest seat, eliminated in reverse fall order) and the
Weng-Lin update applies once. Replay goes back through the queue.

- **Enforced**: ranked ws tests; the once-only property holds because
  both finish paths transition out of `Active` before calling
  `finish_ranked` and `PlayAgain` is rejected.

### S9. A token is verified on every auth-carrying message (ADR-0009/0037)
`create` / `join` / `spectate` / `queue_ranked` / `get_rating` all go
through `ws::authenticate`, and `exp` is enforced there via the single
`auth::is_live` (60s `CLOCK_SKEW_LEEWAY_SECS`, shared by the EdDSA and
HS256 verifiers - the constant's doc comment carries the RFC 7519 4.1.4
reasoning and is the only place to change it). Never relax this for a
"rejoin" - holding a seat is not proof of identity. The corollary lives
on the client (C6): keeping a session alive is the CLIENT's job, by
renewing the token.

The credential is an OIDC **ID token**, deliberately (ADR-0009 amendment
2 - it is the only token guaranteed to be a JWT carrying profile claims,
which is what keeps verification stateless and offline). That choice
carries one obligation: its `aud` is the *client* id, so
`--identity-audience` is the only claim asserting the token was minted
for Parcello at all. Treat it as required; its absence is warned at boot.

- **Breaks**: an expired or stolen token walking into a room; without the
  `aud` check, any token the issuer ever signed - including one minted
  for an unrelated application sharing that issuer.
- **Enforced**: `auth::tests::expiry_allows_clock_skew_but_not_dead_tokens`,
  `tampered_or_expired_tokens_are_rejected`, the eddsa expiry test,
  `eddsa::tests::audience_is_enforced_when_configured`.

---

## C. Clients (Flutter, CLI)

### C1. Every visible Flutter string is an ARB key - in BOTH files
`lib/l10n/app_en.arb` (template) and `app_fr.arb`. Never a literal in a
widget. The generated `app_localizations*.dart` is gitignored.

- **Enforced**: `flutter gen-l10n` fails on template keys missing
  translations only at lookup; review both files on every string.

### C2. Colours live in `tokens.dart`; durations in `motion.dart`
A hex or duration literal at a use site is a bug (docs/motion-language.md,
ADR-0030). `director.compile()` is pure - no socket, no widgets, no
clock - and the animation budget is decided there, checkably
(test/director_test.dart).

### C3. Formula mirrors must stay in sync
Net worth (`GameState::net_worth` <-> `session.dart::netWorth`) and
market price (`Exec::market_price` <-> `protocol.dart::marketPrice`) are
deliberately duplicated for display; a comment at each site points to
the others. Victory points are deliberately NOT duplicated - the server
computes `PlayerView.victory_points` in the view (ADR-0020, "the lesson
of the thrice-duplicated net-worth display").

### C4. Dart null-comparison trap around seats
`s.seat` is `int?`; a spectator has `seat == null`. Any
`something == s.seat` where the left side can also be null (e.g.
`TileState.owner`) silently matches for spectators. Guard with an
explicit `if (s.seat == null) return false;` first - see
`GameScreen._hasTileActions` for the precedent and the comment.

### C5. The layout floor is 1024x600, and a pumped overflow is a failure
`test/layout_test.dart` renders the loaded game screen at the three
shipped sizes; new persistent UI must participate in layout (live in a
scrolling panel), not float over the board where it can cover tappable
tiles and where overflow tests cannot see it. (Coach marks moved into
the side panel for exactly this reason, 2026-07.)

### C6. The client keeps the credential alive; nothing persists a refresh token
`AuthManager` is the ONLY holder of the OIDC grant (ADR-0037). It renews
the id_token before `exp` - a timer ~120s ahead, plus a lazy check on
every use, because timers do not fire on a suspended machine - and every
auth payload is built from `freshIdToken()`, never from a value captured
at startup. The refresh token stays in memory: never in
`reconnect.json`, never in `localStorage`, never in a log line. A socket
that drops without being asked to is retried and the room re-entered
automatically; a deliberate close is not.

- **Breaks**: a session that dies at the issuer's token lifetime -
  mid-game - and a long-lived credential sitting in plaintext storage.
- **Enforced**: `test/oidc_test.dart` (renewal policy, single-flight,
  rotation, `clear()`), `test/session_reconnect_test.dart` (drop ->
  rejoin, fresh token on the rejoin, deliberate leave not undone).

---

## X. Cross-cutting

### X1. Toolchain and hygiene
MSRV Rust 1.96; build/test `--locked`; clippy pedantic+nursery under
`-D warnings`; `unsafe_code = "forbid"`; the allow-list in the workspace
`Cargo.toml` is the only sanctioned lint escape hatch. Licenses:
permissive only (commercial distribution is planned) - `cargo deny`
gates it. Plain ASCII in code, filenames, and game data.

### X2. Deviations require ADRs - before the code
`docs/architecture.typ` is the constitution; ADRs 0001..0035 are its
amendments; CLAUDE.md indexes both. A change that contradicts either
without a new ADR is architectural drift, whatever its diff size.

### X3. Derived documentation must declare its sources
`docs/LLM_CONTEXT/*` and this file are *derived* views. Each carries a
pointer to its sources of truth; when a source changes, the derived doc
is updated in the same change or the change is incomplete.

---

## Known enforcement gaps (honest list)

- E1 (engine purity) has no automated check. A `cargo deny` ban list on
  engine dependencies would harden it; until then it is review-only.
- S6's Probe/activity rule is one guard in `Room::run` with a comment;
  a regression test would require virtual-time idle simulation (the
  paused-clock room tests are the place to add it).
- The fuzzer computes its bid floor from the LIST price, which equals
  the market price only because the fuzzer's content has no market
  events (`market_events: vec![]`). Adding market events to fuzzer
  content without switching its floor to the effective price will panic
  the fuzzer under a price *boom*. Documented in
  docs/technical-debt.md; fix belongs with whoever first fuzzes market
  events.
