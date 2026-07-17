# ADR-0021: public market forecast queue

Status: accepted

## Context
The only scheduled content today is card draws on landing -
unpredictable by design. The v2 ruleset wants the opposite lever too: a
public queue of upcoming market events players can plan around.
`docs/business-tour-direction.md` already wished the rent boost
(ADR-0012) could be "tied to a board event"; this is that mechanism.

## Decision
- Mods gain `data/events.toml`: a `[forecast]` scalar block (initially
  just `gap_turns` between events) and a pool of `[[event]]` defs
  `{ id, name, effect, magnitude_pct, duration_turns }`. Calibration is
  data-only: playtests edit TOML, never the engine. `RegistryBuilder`
  merges by id, exactly like tiles and cards.
- Effects, initially the pitch trio:
  - `rent_multiplier`: scales rent in `resolve_landing` (composes with
    ADR-0012 boosts) - e.g. a crash at -50% for N turns;
  - `acquisition_multiplier`: moves the PRICE of a property (amended
    2026-07) - `Exec::market_price` is the single reference, and the
    sealed-bid floor (ADR-0018), the discoverer's implicit bid, the
    `BidBelowFloor` check, the takeover cost (ADR-0022) and the price the
    client prints on the tile all read it - e.g. a bubble discount.

    Superseded: it used to scale the *settlement* instead, leaving the
    floor at list price. That made the board lie - a tile printed "$80"
    during a -20% crash while the engine rejected any discoverer bid under
    $100, then charged $80 anyway. Moving it to the price means the number
    on the tile is the number you may bid, and the crash reads as "the
    ticket got cheaper" rather than as an invisible rebate. The multiplier
    must now be applied in exactly one place: settlement pays the bid
    as-is, because re-applying it would compound (a -20% crash settling at
    -36%).
  - `wealth_tax`: one-shot on activation (`duration_turns = 0`), every
    player pays `net_worth * pct / 100` through the normal
    tax/partial-payment machinery.
- `GameState.forecast` holds the next 3
  `ScheduledEvent { starts_at_turn, duration, event_id }` plus the
  active effect. Drawn from the seeded RNG (ADR-0002): three at game
  start, one more each time one activates, all during turn transitions
  inside `apply` - rolling, deterministic, replay-identical. Events
  chain sequentially, `gap_turns` apart; no overlap in v2.
- The whole queue is public in `ClientView` by design. This reveals
  draws already made, not the generator: the seed and deck order stay
  on the never-expose list. A public roadmap is the point of the
  mechanic.

## Consequences
- Protocol: a view field plus `Event::MarketEventActivated/Expired`;
  clients render a three-slot timeline strip.
- Tests: same seed, same schedule; modifier window edges; the tax
  through the bankruptcy path; TOML merge override of the pool.
- The base mod ships a starter pool (bubble / crash / audit) with
  deliberately rough numbers - calibration is a playtest task and never
  an engine change.
- Bots may ignore the forecast initially (worse play, not broken play).
