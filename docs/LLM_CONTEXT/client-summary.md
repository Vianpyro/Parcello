# Client summary (`clients/flutter` + `crates/cli`)

Sources of truth: clients/flutter sources & README, docs/
visual-identity.md, docs/motion-language.md, ADR-0025/0028/0030/0033,
docs/INVARIANTS.md C*.

## Flutter client (the product)

One Dart codebase for desktop (Win/Linux/macOS) and web (ADR-0025);
NOT in the cargo workspace; the server serves the web build from disk.
Four files split per platform via conditional export
(`dart.library.js_interop`): oidc login, lan_discovery, server_manager,
session_storage.

**State**: `GameSession` (ChangeNotifier; connection + projected truth)
vs `StageState` (what the board is SHOWING - a separate notifier so
animation frames never repaint input fields). `seat == null` +
`spectating == true` = watcher mode (C4: guard null-seat comparisons).

**Screens**: ConnectScreen (probes the typed server's /config.json -
liveness line + guest-path gating when `guest_allowed:false`) ->
MenuScreen (card grid: private table create/modded/join, Watch a game,
LAN + server manager on desktop, rules; replay-tips button) ->
GameScreen (board with centre HUD built once per update; scrolling
side panel: seats, room, trades, settings, post-game survey, spectator
badge, coach marks).

**Motion** (ADR-0028/0030 + motion-language.md): Updates queue;
`director.compile(events, ctx)` is PURE and produces a Plan whose cost
is budget-checked (tiered 8/6/4s) before the first frame; `session`
executes, applies the authoritative view, then acks
`animation_done{seq}`. Escape skips (state never lost, only its
journey). All colours in `tokens.dart`, all durations in `motion.dart`.

**Onboarding**: five contextual coach marks (lobby/hand/auction/jail/
VP), one at a time, first-occurrence, persisted under the `_hints`
reserved key, reset from the menu; spectators get none.

**Localization**: gen-l10n; EVERY visible string is a key in BOTH
`app_en.arb` and `app_fr.arb`; generated files are gitignored;
`describeEvent` localizes the log.

**Layout floor**: 1024x600; `test/layout_test.dart` pumps the loaded
game screen at three shipped sizes - overflow = failure. Persistent UI
belongs in scrolling panels, not board overlays.

**Controller/Deck**: focus-based navigation; board tiles focusable only
when actionable; Escape = back.

## CLI (`crates/cli`) - the test harness, not a product

Keep it in sync with every protocol change (cheapest end-to-end
check). Modes: `--create` / `--join CODE` / `--queue` (ranked; auto-
joins on match_found) / `--spectate [CODE]`. `--bot` = autopilot via
the shared engine heuristic; stdin commands cover the full protocol
(start/addbot/set/mods/play/route/bribe/vote/bid/build/trade/feedback/
rating/cancel-queue...). The `discover` bin listens for LAN announces.

## Sync duties when the protocol changes

protocol.dart (+ describeEvent + director beat if visual) and CLI
(input.rs/ui.rs) BOTH move in the same change - CLAUDE.md treats an
out-of-sync client as an incomplete diff.
