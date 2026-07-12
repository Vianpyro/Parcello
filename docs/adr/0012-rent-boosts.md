# ADR-0012: rent boosts (pay to raise a tile's rent)

Status: accepted (amended 2026-07: boosts are one-shot - the first rent
collected at the boosted rate consumes the whole boost, whatever its
level, announced by `Event::RentBoostConsumed`. A boost is now a trap you
arm, not a permanent upgrade; playtests showed permanent boosts
snowballed too hard for their cost.)

## Context
Another Business-Tour lever for swingy games: paying to multiply a city's
rent (its "championships"), so a well-placed tile can suddenly hurt. This
is distinct from building houses (which needs a full group and even
building); a boost applies to a single tile you own.

## Decision
A new command `BoostRent { tile }` raises an owned, unmortgaged tile's rent
by one step, on the owner's turn (AwaitRoll/AwaitEnd). Gated by
`rules.rent_boost` (a cost percent per boost; 0 = disabled, default off;
the base fast mod sets 100).

- Cost per boost = `price * pct/100`.
- Each boost adds `RENT_BOOST_STEP_PCT` (50%) to that tile's rent, capped at
  `MAX_RENT_BOOSTS` (3) -> up to +150% (x2.5). The multiplier is applied in
  the engine after the `RentCalculator` computes the base rent, so it
  stacks on top of monopoly/house/scaled rent without touching the strategy.
- Boost level lives in `TileState.boosts` (serde-defaulted, so it is in the
  `ClientView` for free) and **resets whenever the tile changes hands**
  (purchase-decline auctions aside: expropriation, trade, bankruptcy all
  clear it). A new owner starts from zero.
- Rejections (`RentBoostDisabled`, `BoostLimit`, `AlreadyMortgaged`,
  `NotOwner`) never mutate (ADR-0001).

## Consequences
- New `CommandError` variants and `Event::RentBoosted { player, tile,
  boosts, cost }`; all three clients render it and show a `⚡N` badge.
- The net-worth formula (ADR-0010) does NOT count boost spending as equity -
  a boost is a sunk cost, like the premium in a rent payment. Boosting
  therefore lowers your net worth by its cost; that is intended (it is an
  aggressive bet, not a store of value).
- Rent is now `base * (100 + 50*boosts)/100`; the one place rent is charged
  (`resolve_landing`) applies it. If a new rent path is ever added, apply
  the boost there too.
