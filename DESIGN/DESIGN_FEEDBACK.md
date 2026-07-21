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
