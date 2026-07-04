# ADR-0013: domination win (control N full colour groups)

Status: accepted

## Context
Business Tour offers multiple win conditions beyond bankrupting everyone;
one is controlling all the cities of several colours. Parcello already has
last-player-standing and the time-limit net-worth win (ADR-0010); this adds
an instant "domination" win so a strong board position ends the game
without waiting for bankruptcies.

## Decision
`rules.win_full_groups = N` (i64, 0 = off): the first player to own **N
complete colour groups** wins instantly. Checked in the engine after every
accepted command (`Exec::check_group_win`), using
`GameState::full_groups_owned`. Mortgaged tiles still count for ownership
("control", not "cash out"). Ties (a single trade can complete a group for
one party and break another's) resolve to the lowest seat. Emits
`Event::WonByGroups { winner, groups }` and sets the game Finished.

The base fast mod sets `win_full_groups = 3`. Default is off, so the
`classic` mod and bare `RuleParams` are unaffected.

Why only this condition, and not "own all resorts" or "own a whole side"
from Business Tour:
- "Own all resorts" needs the engine to know a specific group name; rules
  are i64-only today, so a `win_group = "<name>"` string rule would first
  require extending the rule registry. Deferred.
- "Own a whole side" needs ring geometry, but the engine is board-layout
  agnostic (the ring is a client concept; the engine sees a flat array).
  Encoding sides would leak layout into the engine. Deferred.
`win_full_groups` is the clean, general, board-agnostic condition and
covers the headline "control all cities of N colours".

## Consequences
- Determinism/replay unchanged: the check is a pure function of state run
  inside `apply`, so it is part of the normal command result (unlike the
  server-clock time win, ADR-0010). The `same_seed` replay guard still holds.
- New `Event::WonByGroups`; all three clients render it.
- Synergy with expropriation (ADR-0011): you can force-complete a group and
  win, which is exactly the intended "nervous" swing.
