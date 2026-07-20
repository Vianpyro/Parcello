# Screen architecture

Reusable RULES per screen, not pixel layouts. For each: purpose,
primary action, information hierarchy, navigation, responsive/empty/
error/loading behaviour, accessibility. The client's actual screens
(`clients/flutter/lib/ui/`) implement these; when they diverge, either
the screen or this file has a bug - reconcile via DESIGN_REVIEW.

The app is a linear spine with one branch:

```
Connect -> Menu -> { Private lobby -> Game -> Finished }
                  { Ranked queue -> Game -> Finished }
                  { Spectate -> Game(watch) }
                  { Rules }
```

## Connect

- **Purpose**: choose a server + identity with zero anxiety.
- **Primary**: Connect (or Sign in when guests are off). **Secondary**:
  Sign in, language.
- **Hierarchy**: wordmark > server URL (+ liveness line) > name >
  auth > connect.
- **States**: probing (silent), reachable/unreachable line, guest-off
  (promote Sign in, disable Connect WITH the reason caption). **Loading**:
  the probe never blocks; Connect works before it returns.
- **Responsive**: single centered card, ~380 px, scrolls on short
  viewports. **A11y**: focus order top-to-bottom; the reason caption is
  the accessible copy for the disabled state.

## Menu

- **Purpose**: pick a path; feel invited.
- **Primary**: Private table (Create). **Secondary**: Watch, Rules,
  ranked/coming-soon (dashed), LAN/server-manager (desktop only),
  replay tips, disconnect.
- **Hierarchy**: card grid, primary card visually heaviest; coming-soon
  quiet and dashed (honest promise). **Empty/loading**: none - the menu
  is static.
- **Responsive**: `Wrap` grid, centered, max ~680 px; tiles reflow, never
  overflow. Desktop-only tiles simply absent on web (not disabled).
- **A11y**: `FocusTraversalGroup`, directional D-pad traversal; every
  tile focusable with a gold ring.

## Lobby (pre-game, host + joiners)

- **Purpose**: assemble a table.
- **Primary**: Start (host, >=2 seats) / for joiners: wait. **Secondary**:
  add/remove bot (host), settings (host edits, all see), copy code,
  back to menu, resign/leave.
- **Hierarchy**: room code + seat list (the table filling) > start/bots
  > settings summary > trades placeholder.
- **Empty**: "Waiting for players..." on the board area + the first coach
  mark. **A11y**: settings summary is text ("game 60 min - turn 25 s -
  bank 45 s"), not icon-only.

## Ranked queue

- **Purpose**: wait with a heartbeat.
- **Primary**: cancel. **Hierarchy**: state ("searching...") > queue
  size > what changes the wait. **States**: queued (size updates),
  match_found (-> auto-join), aborted (message + re-queue handle).
  **Loading IS the screen** - so it must show progress, never a bare
  spinner. **A11y**: size updates announced; cancel always reachable.

## Game (the board) - the load-bearing screen

- **Purpose**: read the table, decide, act, in 12-second turns.
- **Primary action is CONTEXTUAL and singular**: play a card / submit a
  bid / vote / choose a jail exit / end turn - exactly one is the live
  decision, and the board's staging (lift/recede/anchor) points at it.
- **Information hierarchy** (fixed positions - spatial constancy is the
  cognitive aid): board center = protagonist; your hand = hero, bottom;
  per-seat cash+VP = always visible; the contested tile carries its own
  clock; the side panel = receipts, trades, log, settings; banners =
  board-center, one at a time.
- **Navigation**: tiles focusable ONLY when actionable (A/D-pad opens
  the action sheet); Escape skips the animation in flight (never leaves
  the screen mid-game); leave/resign are explicit, in the panel.
- **Responsive**: fixed composition board + 340 px panel + 12 px
  gutters; the FLOOR is 1024x600 (layout-test invariant); below it the
  board center cannot hold the HUD - that is the floor, not a bug.
- **Loading/reconnect**: on join mid-game, snap to truth + ONE 900 ms
  re-orientation (your pawn pulses, acting seat lights) - never a
  catch-up replay (motion-language 9). Disconnected: board greys 20%
  so a frozen board never reads as a slow turn.
- **Error**: a rejected command shakes the SUBJECT and prints the reason
  ON it - never a modal, never only a log line.
- **Watch variant (spectator)**: same screen, no action panel, a badge,
  every P2 demoted to P3; the only control is Leave.
- **A11y**: everything an animation says is also static (band=owner,
  panel=cash, log=sentence); reduced/instant motion honoured; no
  action gated behind hover.

## Finished

- **Purpose**: witness the result, choose what's next.
- **Primary**: Play again (plain) / back to queue (ranked). **Secondary**:
  survey (side card, once, optional, non-blocking), continue to menu,
  ratings delta (ranked).
- **Hierarchy**: winner (ceremonial, Fraunces) > standings (+ ranked
  deltas) > next-action > survey. **A11y**: the win is stated in text,
  not conveyed only by the recede/rise motion.

## Rules

- **Purpose**: teach the shape of the game to the diligent.
- Static, scrollable, sectioned (Goal / Moving / Auctions / Building /
  Jail / Winning), Fraunces headings. Pops on Escape/controller-B.
  Complements coach marks (contextual) - this is the reference, they are
  the just-in-time. **A11y**: plain reading order, text-scalable.

## Cross-screen rules

- One primary action per screen state; it is always the most prominent
  gold element.
- Every dead end returns a handle (aborted queue -> re-queue; error ->
  the subject + reason; disconnect -> reconnect/menu).
- No screen reflows on a state change; state restyles in place.
- Loading is never a bare spinner: show what is awaited and what changes
  it.
- Empty states teach the next action (lobby waiting -> the coach mark;
  no trades -> "No open offers.").
