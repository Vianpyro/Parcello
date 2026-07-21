# Design feedback - what real-screen migrations teach us

The Design System is only as good as the screens it can actually build. A
component "works" in the Showcase; it is only *validated* once a real screen
depends on it (the owner's rule: **no component is considered finished until
it has been used in at least one real screen**). This document is where each
screen migration reports back: what the system covered cleanly, where it fell
short, and - classified - what the shortfall was.

The objective of a migration is not merely to change the screen. It is to
**validate (or falsify) the Design System**. A migration that finds nothing is
either trivial or not looking hard enough.

## Friction taxonomy

Every place the system was insufficient is filed as exactly one of:

- **missing token** - a value existed inline because no token names it.
- **missing component** - a widget had to stay legacy; the system has no
  equivalent yet.
- **missing motion primitive** - an animation had no `Motion` duration/curve
  or director beat to express it.
- **API issue** - the component exists but its surface made the call site
  awkward, forced a workaround, or couldn't express the need.
- **layout issue** - the component didn't compose into the screen's layout
  cleanly (sizing, overflow, alignment).
- **typography issue** - no `PcText` role fit, or a role forced the wrong
  size/colour.
- **visual issue** - the result looked wrong (register, contrast, weight)
  even though it compiled.

Findings are recorded, **not all solved on the spot** - solving in-flight
would bias the screen toward whatever is easy, and hide the gap. Action items
are prioritized at the end of each entry and fed back into
`COMPONENT_INVENTORY.md` / `IMPLEMENTATION_ROADMAP.md`.

---

## Migration #1 - Connect Screen (the reference implementation)

`lib/ui/connect_screen.dart` - step 1 of the client: pick a server + identity.
Chosen as the first migration because it is small, self-contained, exercises
buttons + surfaces + inputs + a dialog + conditional/disabled states, and every
future screen will copy its shape. **This screen is now the reference: future
screen migrations should look like it.**

### What the system covered cleanly (validation wins)

- **PcButton** carried the whole action area. Both real behaviours it was
  designed for showed up and worked:
  - the **variant switch** - when the server refuses guests, sign-in becomes
    `primary` and Connect is the disabled one; otherwise sign-in is
    `secondary`. One conditional on `variant:`, no bespoke styling.
  - **disabled-with-reason** - Connect's "this server does not accept guests"
    caption used to be a *separate* conditional `Text` above the button plus a
    hand-rolled `onPressed: null`. Both folded into a single
    `disabledReason:` prop. The system **removed** a moving part.
  - full-width in a `crossAxisAlignment: stretch` column: `wide` height +
    parent stretch gave the old `wideButton` look exactly.
- **PcCard** replaced the outer `Card`. Padding via the `padding:` prop; the
  380 px width constraint stayed *outside* the card (a `SizedBox`), which is
  correct - width is the screen's concern, not the container's.
- **PcText.wordmark** (title) and **PcText.caption.copyWith(color:)** (the
  reachability line, muted/oxblood by state) - the role + override pattern was
  enough for the sized text.
- **Tokens**: the screen was already fully token-clean; the C2 guard stayed
  green through the migration. No `missing token` findings.
- **Net dependency drop**: the screen no longer imports `ui/common.dart`
  (`wideButton` was its only use). The DS is *replacing* the ad-hoc layer, not
  sitting beside it.

### Frictions found (classified)

| # | Classification | Finding |
|---|---|---|
| F1 | **missing component** - **RESOLVED** | The three inputs (server URL, display name, and the issuer field in the sign-in dialog) had to stay raw `TextField`. `PcTextField` (inventory #6) did not exist yet. This is the single biggest gap: a *form* screen is mostly inputs, and the DS can't dress them. **Resolved (A1):** `PcTextField` was built next and the three inputs migrated to it - the finding directly reordered the build plan and the component is now validated on Connect. |
| F2 | **missing component** - **RESOLVED** | The sign-in flow is an `AlertDialog` with a `TextField` + two buttons. `PcDialog` (inventory #9) did not exist, so the dialog stayed fully legacy (Material title/buttons, not PcButton). **Resolved (A2):** `PcDialog` was built right after PcTextField and the sign-in dialog migrated to it (title + field + cancel/confirm, PcButton actions on a raised surface). Connect no longer has any legacy widget. |
| F3 | **typography issue** + **API issue** - **partly RESOLVED (A3)** | The subtitle and the `loginMessage` line are `TextStyle(color: Pc.textMuted)` with an **inherited** size. No `PcText` role fits: every role carries a size (DDR-0018), and there is no "muted text at the ambient size" role. Root cause: the theme set **no default text colour**. **A3 fixed the root cause** - the theme default ink is now explicit `Pc.text`, which unblocked migrating every size-only style to its role and enforcing the `fontSize` C2 guard. The narrow residual (a role for "muted at the *ambient* size") stays open, but it is now a rare case, not a systemic gap - these two lines remain bespoke `TextStyle(color: Pc.textMuted)` and that reads as intentional. |
| F4 | **visual issue** (expected/positive) | `Card` -> `PcCard` dropped the Material elevation shadow: the connect card is now **flat**. This is the correct register (ART_DIRECTION forbids card shadows) and the reference screen now *demonstrates* flat - but it is a visible change, not a value-preserving swap (as PcCard's own doc warns). Recorded so it is a decision, not a surprise. |
| F5 | **API issue** (minor) | `PcButton.disabledReason` renders the reason **below** the button; the legacy caption sat **above** it. Here that is fine - arguably better, since it now sits under the control it explains - but the component fixes the position; a screen that needed the reason above could not ask for it. Accepted as-is; noted in case a second screen disagrees (the "two consumers" bar for changing it). |
| F6 | **observation, not a friction** | A static form exercises **no motion**. The reachability line flips in/out on a hard `setState` with no fade. A P4 ambient fade (`AnimatedOpacity` at `Motion` ambient) would be a nice touch, but the screen never had one and this migration is value-preserving, so none was added. Consequence: **motion tokens/primitives cannot be validated by this screen** - that validation must come from an animated screen (the board / auction). |

### Decisions taken during the migration

- **`disabledReason` is the canonical "why is this control off" pattern.**
  The reason moves under the control. Adopted for the reference; future
  screens follow suit rather than hand-rolling a caption.
- **Bare-colour / ambient-size text stays bespoke** (F3) rather than being
  forced into a mis-sized role. Not solved here - it waits on the
  theme-default-colour decision (below), which is the clean fix.
- **The card goes flat** (F4). No opt-out was added ("raised card with a
  shadow" is not a register we want; PcCard has none deliberately).

### What this taught us about the Design System (the point)

1. **The DS covers actions and surfaces; it does not yet cover the two things
   a screen is mostly made of - inputs and dialogs.** PcButton + PcCard are
   proven, but the reference screen could only be *partially* migrated: three
   inputs and a dialog stayed legacy. The migration re-confirms that
   **PcTextField and PcDialog are the true critical path**, ahead of the more
   decorative chips/badges - a real screen needs a text field far more often
   than a status pill.
2. **The typography role system has a concrete hole**: "muted text at the
   ambient size." It surfaced twice on one small screen (F3). The cleanest fix
   is not a new role but **setting the theme's default text colour to
   `Pc.text`**, after which bare-size roles are safe and ~14 size-only styles
   across the app can migrate. This is now an owner decision with a real cost
   of *not* deciding, measured on a real screen.
3. **`PcButton` is over-delivering**: it absorbed a manual disable + a separate
   caption into one prop. That is the sign of a primitive designed at the right
   altitude. Nothing about its API needed to change to build this screen -
   first real evidence the DDR-0019 freeze was safe.
4. **Motion is unvalidated by design work so far.** Every component to date is
   static, and so is this screen. The system's motion layer (`Motion`,
   director beats) has *no* real-screen proof yet. That is fine for now, but it
   means the motion tokens are the least-validated part of the DS - a known
   blind spot until an animated surface migrates.

### Action items (prioritized - fed to the inventory, not solved here)

- **A1 (high) - build `PcTextField`** (inventory #6). **DONE.** Unblocked F1:
  the three inputs migrated; the component is frozen (DDR-0019) and validated
  on Connect. The migration promoted it ahead of the decorative primitives
  (PcHairline/Chip/Badge), which are now deferred - a real-screen finding
  reshaped the build order.
- **A2 (medium) - build `PcDialog`** (inventory #9). **DONE.** Unblocked F2:
  the sign-in dialog migrated; frozen (DDR-0019) and validated on Connect. Its
  `destructive` variant (the resign confirm) is the documented next-additive.
- **A3 (medium, owner) - set the theme default text colour** (`= Pc.text`).
  **DONE.** Unblocked F3's root cause and the size-only role migration
  app-wide (8 sites -> roles); the `fontSize` C2 guard is now enforced.
- **A4 (low) - leave `PcButton.disabledReason` position fixed** unless a second
  screen needs it above (the two-consumers bar). No change now.
- **A5 (doc) - re-migrate Connect once A1/A2 land.** **DONE.** With PcTextField
  and PcDialog built and adopted, Connect is now **fully DS-native**: no legacy
  `TextField`, no ad-hoc `AlertDialog`, and the `ui/common.dart` + `sfx.dart`
  imports are gone. It is the complete reference other screens copy.

### Outcome

This migration did its job: it **drove the build order**. Two findings (F1,
F2) were promoted ahead of three decorative components, both built and both
validated back on this same screen within the pass - the fastest possible
loop from "the system is missing X" to "X exists and a real screen uses it".
The remaining open items are F3 (typography role gap -> owner decision A3),
F5 (accepted), and F6 (motion still unvalidated by a static screen).

### Verification

`flutter analyze` clean; full suite green (**69 tests**, +8 for the two new
components); C2 guard green (the screen carries no raw colour/on-grid
literal/Fraunces); `ui/common.dart` and `sfx.dart` imports removed. Visual:
rendered flat card, variant buttons, and hairline-underline inputs confirmed
via headless screenshot.

---

## Migration #2 - Settings panel (`lib/ui/side/settings_panel.dart`)

The host's per-room settings editor (ADR-0015): an `ExpansionTile` with 17
labelled numeric fields (host) or a read-only label/value list (guests), plus an
Apply button. Chosen as the next screen because it is small, self-contained, and
input-heavy - the ideal stress test for the freshly-built PcTextField.

**This is the first migration under the new strategy** (components are pulled by
a real screen's demand, not built ahead). It worked exactly as intended: the
screen surfaced a concrete gap, and the existing component *grew to meet it*
rather than a speculative new one being spun up.

### What the system covered cleanly

- **A3 already paid off**: the two label columns were plain `TextStyle(fontSize:
  12)` inheriting their colour. A3 (theme ink = `Pc.text`) plus the size-only
  migration had already turned them into `PcText.label` - so on this screen the
  labels were *already* DS-native before the migration began.
- **PcButton** took the Apply action (`secondary`), dropping the `ui/common.dart`
  (`wideButton`) dependency - same clean removal as Connect.

### Frictions found (classified)

| # | Classification | Finding |
|---|---|---|
| D1 | **API issue -> RESOLVED (additive)** | The 17 host fields are dense, numeric, right-aligned, and their label sits OUTSIDE the field (in the row's left column). PcTextField - built for Connect's roomy, labelled form fields - could express none of that: it required a `label` and had no `keyboardType`/`textAlign`/`dense`. **This is the strategy's first real test**, and the answer was to grow PcTextField *additively* (`keyboardType`, `textAlign`, `dense`; `label` loosened to optional) - all backward-compatible (Connect's calls unchanged), so within DDR-0019's "add optional params freely". A second consumer justified the parameters that would have been speculative on day one. No new component. |
| D2 | **missing component (PcListRow) - DEFERRED** | Both the host rows (`Expanded(label) + field`) and the read-only rows (`label ... value`) are the **PcListRow** pattern (inventory #8: leading/title/trailing). It does not exist yet. Not blocking - the rows work as plain `Row`s with `PcText` roles - so per the strategy it stays deferred until a SECOND screen shows the same label/value list (the seat list in the lobby is the likely trigger). Recorded so the pattern is on the radar, not built on one sighting. |
| D3 | **typography issue (minor)** | The ExpansionTile title (`14/w600`) and the read-only values (`12/w600`) use a **medium weight** the role set does not carry - roles are `w400` (body/label) or `w700` (rowTitle/section). Left bespoke (a role would shift the weight). Noted: if `w600` recurs, it argues for a medium-weight role, but one screen is not enough. |

### What this taught us about the Design System

1. **The pull-based strategy holds.** The gap (numeric/dense input) appeared as a
   concrete need on a real screen; PcTextField absorbed it with defaulted params
   that are now *validated*, not guessed. Had we added `keyboardType` et al. when
   PcTextField was first built (as the header even predicted), they'd have been
   speculative; the second consumer turned prediction into evidence.
2. **A3 compounds.** Because the theme ink and size-only migration already ran,
   an input-heavy screen arrived half-migrated for free - the labels needed no
   work. Foundation work keeps paying forward.
3. **PcListRow is the next component the screens actually want** (D2), but the
   discipline says wait for the second sighting. The lobby seat list is the
   expected trigger; if it is, PcListRow gets promoted the way PcTextField was.

### Action items

- **B1 (low) - watch for PcListRow's second consumer** (D2). The lobby seat list
  is the likely trigger; build it then, not now. Keep it deferred in the
  inventory.
- **B2 (defer) - a medium-weight (`w600`) type role** (D3) only if it recurs.
- No new PcButton/PcTextField findings - both frozen surfaces held (PcTextField
  grew only additively).

### Verification

`flutter analyze` clean; full suite green (**70 tests**, +1 for the label-less
dense-numeric PcTextField case); C2 guard green; `ui/common.dart` import
removed from the panel. PcTextField extension is additive - Connect's usage is
untouched and still passes.

---

## Migration #3 - Lobby / side panel (`lib/ui/side/side_panel.dart`)

The right-hand column: in the LOBBY state (`s.view == null`) it is the room
code, seat list, and the start/bot/copy/back controls; the same file also
renders the in-game seat rows, trades, and the end-game/spectating/resign
cards. The lobby was the target; the state cards came with it (same file,
value-preserving).

### What the system covered cleanly

- **5 `Card` -> `PcCard`** (spectating & finished as `raised`, room/trades/
  resign as base) and **7 `wideButton` -> `PcButton`** (start, add/remove bot,
  copy-code, back, play-again, continue). Primary/secondary mapped straight
  from the old `primary:` flag.
- **`PcDialog` REUSED** for the resign confirm - its second real consumer, and
  the concrete trigger for the `destructive` param (below). One dialog
  component now serves sign-in (Connect) and resign (side panel).

### The one component change (pull-based, on the 2nd sighting)

| # | Classification | Finding |
|---|---|---|
| D1 | **API issue -> RESOLVED (additive)** | The resign confirm is the SECOND dialog (after Connect's sign-in) and it is destructive. That was the documented trigger to add `PcDialog.destructive` (a defaulted bool -> the primary renders as the destructive PcButton). Added, showcased, tested, and the resign dialog migrated. A second consumer turned a predicted param into a built one - the strategy working exactly as on PcTextField/Settings. |

### Frictions recorded (NOT solved - pull-based restraint)

| # | Classification | Finding |
|---|---|---|
| D2 | **missing variant (API) - DEFERRED** | The resign TRIGGER button is a restrained outlined-oxblood `OutlinedButton` (an always-visible control - it must not shout). PcButton's `destructive` variant is FILLED red - too loud for a persistent button. There is no "outlined/quiet destructive" variant, so the trigger stays bespoke. One sighting; not built. If a second restrained-destructive control appears, that is the trigger for the variant. |
| D3 | **PcListRow / PcBadge NOT triggered - correctly deferred** | The owner flagged these as likely second-sightings. They were NOT: the seat "tags" (you/bot/jail/offline) are **inline text appended to the name**, not pill badges - turning them into `PcBadge`s is a REDESIGN, not a value-preserving migration. The seat rows are the richer **SeatTile** shape (marker + pawn/VP anchor + cash + VP + bid-reveal), and the trades are the **TradeOfferCard** shape - both L3 domain composites, not generic list rows. So `PcListRow`/`PcBadge` stay deferred; building them here would have been the speculative move the strategy forbids. **This is the discipline succeeding, not a gap.** |
| D4 | **minor behaviour note** | `PcDialog`'s Cancel just pops; the old resign dialog played a `buttonNo` earcon on cancel, now dropped (the confirm's `buttonYes` is kept via `onPrimary`). Accepted - the audio set is still placeholder (roadmap Phase 8). If cancel earcons matter later, `PcDialog` gains an optional `onCancel` (additive). |

### What stayed legacy (by design, deferred to domain components)

The side panel is **not** fully DS-native (unlike Connect): the **seat rows**
(-> future `SeatTile`, #13), the **trades list** (-> `TradeOfferCard`, #14),
the resign **trigger** (D2), and the copy `IconButton` remain. The lobby
*controls* are DS; the *game composites* wait for their L3 components. This is
the honest state: control chrome migrates early, domain widgets later.

### What this taught us

1. **The pull-based strategy resisted a speculative build.** The obvious move
   was to "build PcBadge/PcListRow for the lobby". Looking closely, neither was
   a value-preserving second sighting - so neither was built. The rule earns
   its keep precisely when it says *don't*.
2. **PcDialog is now proven across two very different confirms** (a prompt with
   a field; a bare destructive yes/no). Its minimal surface held; only an
   additive `destructive` was needed.

### Action items

- **C1 (defer) - outlined/quiet destructive PcButton variant** (D2): build on a
  second restrained-destructive sighting, not now.
- **C2 (watch) - `SeatTile` (#13) and `TradeOfferCard` (#14)** are the side
  panel's remaining legacy; they are L3 domain composites and the natural next
  build once the primitives are all proven (the game screen migration will
  demand them).

### Verification

`flutter analyze` clean; full suite green (**71 tests**, +1 destructive-dialog
case); C2 guard green. The migrated side panel is exercised by real tests:
`spectate_and_hints_test` renders the migrated spectating `PcCard`, and
`layout_test` pumps the full panel (6 seats, 6 trades) at all three shipped
sizes without overflow.

---

## Migration #4 - Game HUD, first domain component: `SeatTile`

New phase (owner directive): stop chasing residual Material widgets; **build the
BUSINESS components that give Parcello its identity**, and let real game surfaces
pull them. The Game HUD's richest, most-repeated, most Parcello-specific element
is the **seat row** - identity, live cash, victory points, net worth, turn /
bankruptcy / VP-rank state, the chit target, the sealed-bid reveal slot. It was
the standing legacy (`#13`, migration #3's C2 watch item). So the first domain
component is **SeatTile** (`lib/design/components/seat_tile.dart`).

### Design decision - the DS boundary for a domain component

The seat row is entangled with `GameSession` and the stage. A naive SeatTile
would take the whole session. Instead it is **presentational**: it takes
already-resolved, already-**localized** strings (cash, VP label, net-worth
label, joined status tags), plus the stage-owned `anchorKey` (the chit target,
on the pawn circle) and an optional `trailingBid` widget. It imports neither
`session.dart` nor `l10n` - INVARIANTS C1 keeps localization in the app layer.
The parent (`_players`) still computes the cross-seat bits (VP rank, the round
metronome) and formats the labels; SeatTile only draws. This keeps it testable
with plain values (5 unit tests, no session) and correctly layered.

### Pull-based restraint (what was NOT built)

| Classification | Finding |
|---|---|
| **PcBadge NOT pulled** | The inventory listed `PcBadge`/`MoneyChit` as SeatTile deps. Neither was built: the status tags are **inline text** (`(you) (bot)`), and turning them into pill badges is a REDESIGN, not a value-preserving migration - so SeatTile v1 keeps text tags (recorded as Visual Debt VD-1, the gap to the Bible's badge vision). The chit is the overlay's travelling widget; SeatTile is only its TARGET (the `anchorKey`), so `MoneyChit` is pulled by an overlay migration, not this one. Building either here would have been speculative. |
| **Additive `Motion.stateFade`** | The row's 200 ms highlight/dim was an inline `Duration` - a motion-layer bug (`motion.dart` owns every duration). Added `Motion.stateFade` (200 ms, "a UI element changing state in place") additively. A real component surfaced the missing token. |

### Internal type consolidation

The bespoke tabular styles the row used inline (cash, the gold VP figure) moved
INTO SeatTile as `PcText.amount` / `PcText.amount.copyWith(...)` - the component
now owns its type, so the figures are consistent by construction instead of
re-specified at the one call site.

### What this taught us

1. **A domain component can be presentational without leaking the session.**
   Passing formatted strings + the two stage handles kept SeatTile inside the
   DS boundary and unit-testable - the pattern the remaining domain composites
   (PropertyCard, TradeOfferCard) should follow.
2. **The identity gap is now visible, not hidden.** Text-tags-vs-badges is
   logged as Visual Debt rather than silently "fixed" mid-migration - the
   pull-based rule again choosing *don't* until a real decision/screen calls.

### Action items

- **D1 (watch) - `MoneyChit`** is pulled by an overlay migration (the chit's
  static face), **`PropertyCard`** by the board/tile-detail, **`TradeOfferCard`**
  by the trades list, **`AuctionWidget`** by the sealed-bid input. None built
  until their surface migrates.
- **D2 (owner) - decide tags-as-badges** (VD-1): if the Bible's badge vision is
  the target, that decision pulls `PcBadge` and reshapes SeatTile's tag slot.

### Verification

`flutter analyze` clean; full suite green (**76 tests**, +5 SeatTile);
**`layout_test` green at all three sizes** (SeatTile IS the seat row, so this is
the load-bearing check - a loaded 6-seat panel must not overflow a Deck); C2
guard green.

---

## Migration #5 - ActionsPanel chips: `PcChip` (blocker-driven)

Methodology sharpened (owner): pick the next component NOT by business/visual
importance but as **the one whose absence blocks the clean migration of the next
real screen**; among candidates, prefer the one that maximizes immediate reuse.

The next screen is the rest of the Game HUD - **`ActionsPanel`**. Surveying it,
the one thing with NO design-system equivalent is the **Legal Route builder
chips** (`_routeChip`): a tap-to-order TOGGLE chip (gold when picked, showing its
order `#2`), a bespoke `OutlinedButton`. It is not a PcButton (it has a selected
state); nothing in the DS covers it. So it BLOCKS a clean ActionsPanel migration.
And its exact twin is the **mod-picker chip** in the menu (`_modChip`,
`private_table_card.dart`) - identical toggle-order styling. Two consumers on two
different screens => highest immediate reuse. The pull is unambiguous: **PcChip**.

### What was built and migrated

- **`PcChip(label, selected, onTap)`** - built on `OutlinedButton` so it keeps
  keyboard/controller focus + the gold ring (Steam Deck); sharp corners; gold
  fill + gold border when selected, muted hairline when not. Minimal surface.
- Migrated **both** consumers the same PR: `_routeChip` (ActionsPanel) and
  `_modChip` (menu). Each helper collapsed to a `PcChip(...)`; the selection-order
  `#N` badge stays caller-composed (it is the caller's list state).

### Frictions / notes

| # | Classification | Finding |
|---|---|---|
| E1 | **visual (minor densification)** | The route chip was button-sized (h46, via the shared `touch` style); the mod chip was already h40. PcChip standardizes both at a dense h40 / `PcText.label` - so the route chips shrink slightly to the correct chip register (consistency is the point of the shared component). Recorded as Visual Debt VD-10 (accepted register correction, like the flat-card F4). |
| E2 | **scope held** | ActionsPanel still has bespoke ACTION buttons (the `touch`-styled Filled/Outlined buttons for Bid / Play-card / End-turn) and two numeric `TextField`s (bid, bribe). Neither is blocked by a MISSING component: the buttons are a PcButton *sizing decision* (visual - not made speculatively), the inputs need a `PcTextField.inputFormatters` *additive extension*. Both are follow-ons, not this step's blocker - so, per the methodology, untouched here. |

### What this taught us

- **Blocker-driven selection converged on the same answer as reuse-driven would
  have**, but for the right reason: PcChip was chosen because `_routeChip` cannot
  be migrated without it, not because chips are "important". The two-consumer
  reuse was the tie-breaker, not the motivation.
- **The next ActionsPanel step is now a named, small thing**: extend
  `PcTextField` with `inputFormatters` (the bid/bribe fields' only blocker), then
  the button-sizing decision. Neither is a new component.

### Verification

`flutter analyze` clean; full suite green (**79 tests**, +3 PcChip); C2 guard
green; **`bid_input_test` green** (the bid field sits beside the migrated route
chips - its half-typed-bid-survives-a-frame guard still passes), `layout_test`
green.

---

## Migration #6 - the turn action bar is the design system (build-the-game phase)

New phase (owner): stop improving the DS, BUILD THE GAME - one real game screen
per PR, each raising player-perceived quality; a new component only on a real
blocker/2nd consumer. First screen: **`ActionsPanel`** - what the player touches
every single turn (play a card, bid, end turn, jail).

The screen discovered TWO real blockers, both resolved by **additive** growth of
existing components (no new component):

| # | Classification | Finding |
|---|---|---|
| F1 | **API (additive) -> RESOLVED** | The action bar is a `Wrap` of many buttons, needing a compact-but-touch-sized button; PcButton only had `wide` (full-width 52) or intrinsic. Added **`PcButton.dense`** (intrinsic width, 44 touch height, tighter padding). Blocker: the bar could not use PcButton cleanly without it. |
| F2 | **API (additive) -> RESOLVED** | The bid/bribe fields cap digits + amount as you type (a real anti-cheat/UX guard, `MaxValueFormatter`); PcTextField had no way to pass formatters. Added **`PcTextField.inputFormatters`**. Blocker: the fields could not migrate without it. |

Migrated: `btn()` helper, the bid submit, quick-raise (+10/25/50/100%), all-in,
Choose-route, Reset, and the bid/bribe inputs - the whole bar is now PcButton
(dense) + PcTextField (dense, gold-focus hairline) + the PcChip route builder
(#5). The bespoke `touch` ButtonStyle and the raw `TextField`s are gone; the
in-game controls now match the polish of the menus (consistency IS perceived
quality), and the bid field gains the DS hairline + gold focus.

### Notes

- **Value preserved where it mattered**: the mid-edit reseed guard lives in
  ActionsPanel (`_bidInitTile`), untouched - `bid_input_test` (half-typed bid
  survives an animation frame) still passes through PcTextField.
- **`sfx` import dropped** from the panel (PcButton wraps the hover earcon).
- **No screenshot captured this PR**: the bar renders only in a live game; it is
  covered by `bid_input_test`, `layout_test`, the dense-size unit test, and the
  Showcase "action bar" demo (debug builds). An in-game capture is the one
  follow-up if a visual sign-off is wanted.

### Verification

`flutter analyze` clean; full suite green (**83 tests**, +2: dense sizing,
inputFormatters passthrough); C2 + DDR-0020 + spatial-blindness guards green;
`bid_input_test` green.

---

## Design System Coverage (living snapshot - updated each migration)

Maturity ladder: **Experimental** (built + in Showcase, no real screen yet) ->
**Validated** (used in 1 real screen) -> **Stable** (>=2 screens, API held
under additive-only growth) -> **Core** (used in every migrated screen; surface
battle-tested).

| Component | Maturity | Real-screen consumers | API since freeze |
|---|---|---|---|
| `Pc` tokens | **Core** | every widget | additive only |
| `PcText` roles | **Core** | every widget | additive only |
| `PcButton` | **Core** | Connect, Settings, Lobby, Game HUD | grew additively (`dense` for the action bar) |
| `PcCard` | **Stable** | Connect, Lobby (x5 cards) | unchanged |
| `PcTextField` | **Stable** | Connect, Settings, Game HUD | grew additively (numeric/dense/optional-label/inputFormatters) |
| `PcDialog` | **Stable** | Connect (sign-in), Lobby (resign) | grew additively (`destructive`) |
| `SeatTile` | **Validated** | Game HUD / side panel | frozen (new) |
| `Motion` | **Stable (engine)** | director/board/stage + now `SeatTile` (`stateFade`) - but NO migrated *screen* validates it as motion yet (F6) | grew additively (`stateFade`) |
| `PcChip` | **Stable** | ActionsPanel (route builder) + menu (mod picker) | frozen (new) |
| `PcHairline` / `PcBadge` | *deferred* | none - no real screen demands them value-preservingly | not built |
| `PcListRow` | *deferred* | sighted once (Settings rows); awaits 2nd consumer | not built |
| `TradeOfferCard` / `MoneyChit` / `PropertyCard` / `SettingsField` / `AuctionWidget` | *not started* | L3/L4 domain composites, pulled by their surface | not built |

**Screens migrated:** Connect (100% DS-native), Settings (100% of its
controls), Lobby controls (side panel chrome), Game HUD (seat rows ->
SeatTile; the rest of the HUD pending).

**Legacy building-block widgets remaining** (use sites outside the design
system, approx.): raw `Card` x6, raw `TextField` x7, `AlertDialog` x1,
`wideButton` x2, other raw `*Button` x~28 - all in NOT-yet-migrated surfaces
(menu, game HUD, board, trades, feedback, rules). These are the backlog the
next screen migrations retire.

---

## Gameplay Coverage (living - the game's identity surfaces)

Distinct from the component table above (which tracks *primitives*): this tracks
the **game-identity surfaces** - the things a player actually looks at in a match
- and whether each is served by a purpose-built domain component or still ad-hoc.
This is where "Parcello looks like Parcello" is won.

| Gameplay surface | Component | State |
|---|---|---|
| Seat / player card | **`SeatTile`** | **DONE** (domain component, side panel) |
| Sealed-bid reveal chip | `BidChip` (pre-existing, on SeatTile's `trailingBid`) | works; not yet a DS component (candidate to fold in) |
| Property / tile face | `PropertyCard` (#12) | **legacy** - board draws tiles inline; parchment/rent-ladder/mortgaged not componentized |
| Money chit (travelling) | `MoneyChit` (#11) | **legacy** - lives in `overlay.dart`; static face not extracted |
| Trade offer | `TradeOfferCard` (#14) | **legacy** - `_trades()` renders text + TextButtons |
| Sealed-bid INPUT (anchored to the tile) | `AuctionWidget` (#16) | **NOT built** - the #1 UX gap (motion-language 8.2); input is a corner field, clock is a corner number |
| Legal Route builder / mod picker chips | **`PcChip`** | **DONE** (tap-to-order, both screens) |
| Action buttons (Bid / Play card / End turn) | **`PcButton`** (dense) | **DONE** (#6 - the whole action bar is the DS) |
| Bid / bribe numeric inputs | **`PcTextField`** (dense + formatters) | **DONE** (#6 - hairline + gold focus, digits/max capped) |
| VP legend / round metronome | (center panel, bespoke) | ad-hoc; reads acceptably, no component yet |
| Market forecast / pools / spotlight lines | (center panel, `PcText.caption`) | text-only, DS-typed; fine as-is |
| Clocks (turn / bank / bid / vote / game) | `Countdown` (pre-existing) | works; not a DS component |
| Event log | `EventLog` (pre-existing) | works; DS-typed |
| Board tiles / pawns / board motion | `board.dart` (+ director/stage) | rich already; flat by DDR-0017 (a decision, not debt) |

**Read:** the identity backbone is now started (SeatTile). The two highest-value
domain builds left are **`AuctionWidget`** (the signature moment, and the #1 UX
gap) and **`PropertyCard`** (every landing shows a tile). `TradeOfferCard` and
`MoneyChit` follow their surfaces (trades panel, overlay).

---

## Visual Debt (living register - shipped look vs. the Design Bible)

Technical debt tracks code; **Visual Debt** tracks the gap between what ships
today and the vision in the Design Bible (`DESIGN/`, `docs/visual-identity.md`,
`docs/motion-language.md`). An entry is a KNOWN, ACCEPTED divergence - logged so
it is a decision on a list, not a surprise. Severity: **P1** (hurts identity /
readability now) -> **P3** (cosmetic / nice-to-have).

| ID | Area | Shipped today | Bible vision / target | Severity | Owner? |
|---|---|---|---|---|---|
| VD-1 | Seat status | tags are inline text (`(you) (bot) (jail)`) | status **pills/badges** (`PcBadge`), colour-coded | P2 | decision pulls PcBadge (#4/D2) |
| VD-2 | Sealed-bid input | corner field + corner number clock | input **anchored to the lifted tile**, clock a hairline draining on the tile edge (motion-language 8.2) | **P1** | `AuctionWidget` (#16) |
| VD-3 | Property tiles | drawn inline in `board.dart` | parchment `PropertyCard`: group band, rent ladder, mortgaged/conglomerate states | P2 | `PropertyCard` (#12) |
| VD-4 | Trade offers | text lines + plain TextButtons | `TradeOfferCard`: give/receive columns, accept/refuse affordances | P2 | `TradeOfferCard` (#14) |
| VD-5 | Resign trigger | bespoke outlined-oxblood button | no restrained-destructive PcButton variant exists (#3/D2) | P3 | defer to 2nd sighting |
| VD-6 | Muted ambient text | 2 bespoke `TextStyle(color: textMuted)` (Connect subtitle, login line) | a "muted at ambient size" affordance, or accept as intentional (F3 residual) | P3 | after theme pass |
| VD-7 | Off-grid spacing | one-off `3/5/10` insets remain literal | align to the 4-px grid in a visual-review pass, or add tokens | P3 | visual-review pass |
| VD-8 | Rules headings | Inter (were Fraunces) | owner to confirm Inter, or revert to Fraunces if "emblematic" was intended | P3 | owner |
| VD-9 | Audio | placeholder clip set; one cancel earcon dropped (#3/D4) | the four category earcons (AUDIO_DIRECTION) | P2 | roadmap Phase 8 |
| VD-10 | Route chips | densified h46 -> h40 by PcChip (#5/E1) | accepted register correction (chips are dense) - resolved, logged for traceability | P3 | resolved |

**Not debt (deliberate decisions, do not "fix"):** the flat board (DDR-0017),
flat cards (no elevation shadow, ART_DIRECTION), sharp corners everywhere.
