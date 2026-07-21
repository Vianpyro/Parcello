# UX guidelines

The behavioural rules that sit under every screen and component. Where
DESIGN_SYSTEM says what things ARE, SCREEN_ARCHITECTURE says where they
go, and `DESIGN/product/` says what the player does and which information
they need, this says how the interface must BEHAVE toward the player.

## Discoverability

- **Teach by staging, not by tutorial.** The board already lifts your
  hand on your turn and lifts a tile at an auction; coach marks only
  NAME what the staging implies, once, in context (lobby / hand /
  auction / jail / VP). The Rules screen is the reference for the
  diligent; the coach marks are the just-in-time for everyone else;
  watching a bots game is the demo. Three tiers of onboarding, no
  mandatory tutorial gate.
- **Affordances are honest.** A tile is focusable/tappable only when it
  is actionable (C-series invariant); a disabled control shows WHY. The
  player never learns "that looked clickable but wasn't."

## Cognitive load (the 12-second budget)

- One live decision at a time; the staging points at it. Never present
  two competing primary actions.
- Spatial constancy: recurring elements never move (banners center,
  clock on the subject tile, log bottom, seats fixed). A moved element
  costs a re-scan the turn cannot afford.
- Information is generous but LAYERED: the permanent set stays available
  and contextual detail comes on demand (which information is permanent vs
  contextual is owned by `DESIGN/product/INFORMATION_ARCHITECTURE`, not
  here). Density is framed (every zone ruled and titled) so it reads as
  calm, not busy (the CONCEPT_CRITIQUE framing lesson).

## Attention management

- Three escalating devices only (frame < lift < recede); recede ~4x a
  match. Being attacked is one tier louder for the victim than the
  table. The camera never moves - attention is never bought by motion of
  the world, only by staging within it.

## Feedback timing

- Every action gets feedback within ~100 ms (press state, earcon), and
  its CONSEQUENCE animates on the board (the chit, the band). A total
  updates only after its cause arrives (money rule).
- Rejections are immediate, on the subject, with the reason - never a
  silent no, never a modal, never only a log line.
- Nothing important is conveyed only by a transient: things you were
  away for (AFK auto-play, time-bank drain) get PERSISTENT markers.

## Empty states

Teach the next action: lobby "Waiting for players..." + coach mark;
"No open offers." for trades; the unsold-tile strike-through is the
deadpan empty-result of an auction. An empty state is never blank - it
is the smallest possible instruction.

## Loading

Never a bare spinner. Connect probes silently and doesn't block; the
ranked queue shows size + what changes the wait; joining mid-game snaps
to truth then re-orients once. If a wait is unavoidable, show what is
being waited for.

## Error recovery

Every dead end hands back a handle: aborted queue -> "queue again";
rejected command -> the subject shakes + reason; lost connection ->
grey board + non-blocking banner + auto-reconnect with the stored
token. The player is never stranded with no next move.

## Confirmation dialogs

Reserve for the genuinely destructive/irreversible (resign; sign-out).
Everything reversible prefers in-place + undo over a confirm. A confirm
on a reversible action is theatre and trains players to click through
confirms - which then fails them on the one that mattered.

## Input parity (the platform matrix)

Parcello ships desktop, web, and Steam Deck from one codebase; mobile
is future. Every interaction must work on ALL current inputs:

- **Keyboard**: full focus navigation; Escape = back/skip; the primary
  action reachable and activatable.
- **Controller (Steam Input maps to keyboard focus)**: D-pad traverses
  focus groups, A activates, B backs out; visible gold focus ring on
  every focusable; NO reliance on hover; no autofocus on frequently-
  rebuilt panels (it steals focus).
- **Touch (Deck touchscreen now; mobile later)**: min 40 px hit
  targets; no hover-only controls; bottom sheets for tile actions.
- **Mouse/desktop**: hover is an ENHANCEMENT (tooltips, hover earcon),
  never the sole path to anything.
- **Web**: same as desktop minus native-only tiles (LAN, server
  manager) - absent, not disabled; the probe tolerates CORS (unknown,
  never "unreachable").

Rule: if a control works only with one input, it is broken on the
other three. Design for the most constrained (controller/touch) first;
mouse gets the affordances for free.
