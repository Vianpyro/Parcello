# Design system - components

Component-by-component philosophy. For each: purpose, states, variants,
motion tier, accessibility notes, and the mistakes already made once.
Token values live in `lib/tokens.dart`; layout rules in
VISUAL_LANGUAGE.md; this file is the behavioural contract of each part.

The system is small ON PURPOSE. A new component type is a DDR, not a
convenience - the cost of a component is not its code, it is the second
way it teaches the player to read the same thing.

## Global rules (bind every component)

- Radius 2 px; hairline borders; colour-stepped elevation; no shadow but
  the lift shadow. (VISUAL_LANGUAGE.)
- Material = message: dark = machine, parchment = play object, gold =
  value/subject. A component's clothing must answer which it is.
- Every interactive element has: rest, hover (surface step + earcon),
  pressed, disabled (faint, never hidden if the action exists),
  focus (gold ring, actionable-only).
- Localized text only (both ARB files); tabular figures on live numbers;
  no truncation of names.
- State changes restyle in place; they never reflow the layout.

## Buttons

- **Purpose**: commit an action. **Variants**: primary (gold fill, dark
  ink - one per view state, the obvious next step), secondary (dark,
  gold text/border), destructive (oxblood text/border - Resign,
  bankruptcy-adjacent), quiet (text-only, muted - "replay tips",
  cancel). Quick-value chips (auction +50k/+100k/MAX) are a button
  sub-variant: small, secondary, they COMPOSE your sealed bid, they
  are not raises against a public number (CONCEPT_CRITIQUE lesson).
- **States**: pressed = `gold-dark`; disabled keeps shape + shows WHY
  nearby (the greyed Connect precedent), never a dead unexplained
  button. **Motion**: P4 press feedback only; the CONSEQUENCE animates
  on the board, not on the button.
- **Mistakes**: two primaries in one view (there is one next step);
  icon-only at panel scale (pair a label); a confirm dialog where an
  in-place undo would do (see Dialogs).

## Panels

- **Purpose**: group related machine controls/readouts (side panel
  sections, settings). Dark surface, titled with a hairline rule,
  12 px padding. **The side panel is the game's one scroll**: it
  absorbs room growth (trades, log, survey) - the board never scrolls.
- **Motion**: content changes in place; a NEW card (trade offer,
  survey, hint) slides in 150-300 ms easeOut, leaves by dissolve.
- **Mistakes**: putting a play object (a property, a chit) INTO a dark
  panel as chrome - property cards are parchment even inside a dark
  panel; a floating overlay instead of a panel section for anything
  persistent (coach marks learned this - INVARIANTS C5); nesting
  deeper than card-in-panel.

## Dialogs

- **Purpose**: a modal decision the game genuinely must block on -
  which is almost never. Resign confirm and sign-in are the canonical
  legitimate ones. **Rule: prefer in-place over modal.** A rejected
  command shakes the subject; it does not open a dialog. An auction is
  a board mode, not a dialog. The post-game survey is a side card BY
  DECISION, never a modal. **Motion**: establish quickly; a true modal
  is the enemy of a 12-second turn - every proposed dialog must
  justify why it is not an in-place state.
- **States**: one primary + one cancel; Escape/controller-B cancels.
- **Mistakes**: modal auctions/trades (they must be board/panel
  states); confirmation theatre on reversible actions.

## Property card (parchment)

- **Purpose**: the play object. Parchment face, group-colour EDGE BAND
  (never a fill), name in Source Serif 4, rent ladder + price in Inter
  tabular. Owned state = band in owner's pawn colour; mortgaged =
  band desaturates to hatching; conglomerate = gold cap. Market-active
  = effective price with the old beside it (`$72 (was $104)`), tinted
  by valence - and ONLY for price-moving events (acquisition), never
  faking it for rent multipliers (motion-language 8.2, the grammar
  must not lie).
- **Mistakes**: full-colour fill (band only); a dark property card
  (breaks the paper=play-object law); truncating the name.

## Player / seat panel

- **Purpose**: identity + live economy per seat. Avatar/initial, name
  in pawn colour, cash (tabular), VP, connection/bot state, sealed-bid
  dot during a window, jail/route state. **This is the HUD that
  RECEIVES chits** - the seat marker is a motion target, so its
  position is load-bearing (chits fly to it). **States**: acting
  (gold-lit), disconnected (greyed), bot (working pulse during
  BOT_THINK), you ("(you)" tag). **Mistakes**: moving seat positions
  on state change (chits would miss); pawn colour used anywhere it
  isn't identity.

## Auction widget

- **Purpose**: the game's core decision. **Target state (motion-lang
  8.2)**: board recedes, tile lifts, the bid input ANCHORS to the tile,
  the 12 s window is a gold hairline DRAINING on the tile's own edge -
  no corner countdown. Your own bid only; the discoverer's floor shows
  as a ghost value. Abstain always present, unstigmatized. **Current
  build gap**: input still in the centre panel, clock still a number -
  the single biggest spec/build delta (COMMERCIAL_UX_AUDIT). **Never**:
  show any rival's pending bid (E5 - the CONCEPT_CRITIQUE error).

## Money display & chits

- **Purpose**: economy made physical. A TOTAL is Inter tabular, updates
  only AFTER its chit lands. A CHIT is a parchment rectangle carrying
  `+/-amount`, sage-in / oxblood-out, states-then-travels (500+500 ms),
  source->target on the board. **The single highest-value rule**:
  money travels, it is never a delta that ticks (motion-language 4.2).
  **Mistakes**: odometer/rolling totals (casino register); floating
  only the payer's loss (the earner must see the gain - the old defect).

## Notifications (banners / toasts / coach marks / markers)

- **Banner**: board-centre, ONE place, for card reveals / market /
  spotlight - the same object every time (spatial constancy). **Coach
  mark**: one at a time, first-occurrence, in the SIDE PANEL (not over
  the board), dismissible forever, replayable. **Persistent marker**
  (not a toast): AFK-auto-played, time-bank-draining - things you were
  AWAY for cannot be transient (a toast you missed is a bug). **P4
  banner**: connection lost/restored, non-blocking. **Mistakes**: a
  toast for something the player needed to be present to see; two
  banners competing for centre.

## Lists & history

- Event log: sentence-cased, localized, order-is-information, no
  timestamps (times belong to match-history surfaces). Trade list,
  seat list: hairline-under-header, spacing-separated rows, tabular
  numbers right-aligned. **Mistakes**: zebra striping (spacing does
  it); timestamps in the live log (noise).

## Chat

- **Does not exist and is not a UI decision to add.** Chat is a
  moderation surface against the no-admin-plane stance - adding it is
  an ADR (technical) + a DDR (design), never a component someone drops
  in. Documented here so nobody treats the CONCEPT_CRITIQUE mockup's
  chat panel as a backlog item.

## Tooltips & context menus

- **Tooltips are desktop-only** (hover) - never the sole carrier of
  anything (controllers/touch have no hover). **Context menu** (tile
  tap -> build/sell/mortgage/seize): a bottom sheet of the legal
  actions for THAT tile, opened only when the tile is actionable
  (focusable-only-when-actionable, C-series). **Mistakes**: hiding a
  needed control behind hover-only; a menu that offers illegal actions
  (the sheet lists only what the engine will accept).

## Forms (connect, settings)

- Minimal fields, inline validation, the probe as liveness
  (connect screen). Settings = clamped inputs mirrored from the server
  (host edits, all see). **Mistakes**: client-side-only validation that
  the server then rejects (the clamp is authoritative - show the
  clamped value back); blocking connect on a best-effort probe.
