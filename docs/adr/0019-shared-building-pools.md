# ADR-0019: shared building pools (subsidiaries and conglomerates)

Status: accepted

## Context
Build limits are per tile only (`max_houses_per_property`, level 5
renders as a hotel); the bank's stock is infinite. The v2 ruleset wants
a table-wide scarcity that scales with the player count - both an
economic brake and the fuse of the pool-exhaustion game end (ADR-0020).

## Decision
- Renaming, data unchanged: house levels 1..max-1 are "subsidiaries";
  the top level (max, today's hotel) is a "conglomerate". Each level
  still costs the tile's `house_cost`; sell-back still refunds half.
- Two new `RuleParams` scalars, engine default 0 = unlimited (the
  existing off-by-default pattern of expropriation/rent_boost):
  `subsidiary_pool_factor` and `conglomerate_pool_factor`. The base mod
  sets 6 and 3. At `GameState::new`, two global fields are computed
  once: `pool = round(factor * sqrt(players))`:

  | players       | 2 | 3  | 4  | 5  | 6  |
  | ------------- | - | -- | -- | -- | -- |
  | subsidiaries  | 8 | 10 | 12 | 13 | 15 |
  | conglomerates | 4 | 5  | 6  | 7  | 7  |

- `Build` consumes from the matching pool and is rejected (new
  `CommandError::PoolExhausted`) when it is empty. Building the
  conglomerate level consumes one conglomerate and RELEASES the max-1
  subsidiaries the tile held (the classic house-to-hotel conversion) -
  scarcity stays tactical rather than punitive.
- Everywhere houses move, pools move (the even-build/even-sell trio):
  `SellHouse` returns a subsidiary - or returns a conglomerate and
  re-consumes max-1 subsidiaries when stepping down from the top level;
  `StandardLiquidation` returns everything it strips; takeover
  liquidation (ADR-0022) does too.
- Scarcity on the way down: a voluntary `SellHouse` off a conglomerate
  is rejected when fewer than max-1 subsidiaries are free (the bank
  cannot re-lend what it does not hold; mortgaging remains the
  liquidity valve). The FORCED liquidation path instead strips such a
  tile to zero levels in one motion, refunding every level at half
  cost - bankruptcy resolution must always succeed. Playtests may
  soften the voluntary rule later.
- Both pool counters are public in `ClientView`: the tension only works
  if everyone watches the shelf empty.
- Trades never move improved tiles (existing rule), so trading cannot
  touch the pools.

## Consequences
- Protocol: two new public view fields; clients render the two
  counters.
- New tests: pool accounting through build/sell/liquidation/takeover,
  exhaustion rejects, conversion release and re-consumption.
- `bot::decide` should weigh scarcity (racing for the last conglomerates
  is the point); the naive "build when affordable" stays acceptable
  initially.
- ADR-0020 hooks the game end on `conglomerates_available` reaching 0.
