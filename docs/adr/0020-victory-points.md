# ADR-0020: victory points and the pool-exhaustion game end

Status: accepted

## Context
Wins today: last player standing, richest at the time limit (ADR-0010),
domination by full groups (ADR-0013). All are slow or terminal. The v2
ruleset adds the primary mode: a race to a victory-point target where
points mirror the CURRENT board - lose the asset, lose the points - so
hostile takeovers (ADR-0022) stay relevant to the last minute.

## Decision
- New `RuleParams` scalar `win_victory_points` (engine default 0 = off;
  the base mod sets 20 and turns `win_full_groups` off - a 3-group
  domination would short-circuit the race at 9 points).
- Pure scoring on the `net_worth` model,
  `victory_points(state, content, seat)`:
  - +3 per complete colour group (`full_groups_owned`);
  - +2 per conglomerate-level tile
    (`houses == max_houses_per_property`);
  - +1 per resort owned (`RentModel::GroupScaled`; extend the same way
    to any future scaled model);
  - plus `Player.round_bonus_vp`, the only stored term.
- Round bonus: each round, the player with the strictly highest cash
  (tie: lowest seat, the house convention) banks +2 into
  `round_bonus_vp` permanently - an early economic lead leaves a
  permanent mark even if the cash later evaporates. Rounds tick off the
  velocity deck: the round number is the minimum `hands_cycled`
  (ADR-0017) across surviving players, and the bonus fires whenever
  that minimum increases - checked in `apply` after any command that
  refills a hand.
- Victory check after every accepted command, like `check_group_win`:
  reaching the target finishes the game with
  `Event::WonByPoints { player, points }`.
- The pool-exhaustion end ("doom clock"): when a `Build` drops
  `conglomerates_available` (ADR-0019) to zero, first run the normal
  victory check - the builder may have just crossed the target, which
  is a points win - otherwise the game ends immediately and the highest
  score wins, ties broken by `net_worth` then lowest seat:
  `Event::WonByPoolExhaustion { winner }`. Unlike ADR-0010 this is game
  state, not wall clock, so it lives entirely in the pure pipeline. A
  points leader may deliberately buy the last conglomerate to slam the
  door - intended.
- `PlayerView.victory_points` is computed in the view projection so the
  three clients do not re-implement the formula (the lesson of the
  thrice-duplicated net-worth display).
- Richest-at-time-limit (ADR-0010) survives as the backstop for the
  target 10-15 minute session; last-standing survives trivially.

## Consequences
- Protocol: one view field, two events; clients surface VP prominently
  (the race IS the game) plus the conglomerate fuse.
- Tests: each scoring term, reversibility (losing a group loses its 3
  points), round-bonus accrual and tie-break, doom-clock ordering
  against a simultaneous points win, `win_victory_points = 0` mods
  keeping today's behaviour.
- `bot::decide` should eventually chase points (conglomerates and group
  completion first); shipping with the current economic heuristic and
  tuning at playtests is acceptable.
