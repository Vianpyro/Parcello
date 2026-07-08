# ADR-0022: takeover happens on the landing tile (amends ADR-0011)

Status: accepted

## Context
ADR-0011's `Expropriate` targets any rival tile that is unimproved and
unmortgaged, anywhere on the board, at any point of the acting player's
turn (AwaitRoll or AwaitEnd). In the reference game the move is
contextual: you may seize the very tile you just landed on, right after
paying its rent. The v2 ruleset adopts that: less alpha-strike sniping,
more drama on every landing.

## Decision
- Legality tightens to: `phase == AwaitEnd`, `tile == the acting
  player's current position`, tile rival-owned. Rent has already
  resolved automatically at landing, unchanged - the old owner may
  collect both the rent and the takeover compensation.
- Improved tiles become seizable (drops ADR-0011's `houses == 0`
  precondition). Their buildings are liquidated at `sell_house` pricing:
  the OLD owner receives `house_cost / 2` per level on top of the usual
  compensation, and the stripped units return to the shared pools
  (ADR-0019; a conglomerate tile returns one conglomerate). The taker
  always receives a bare tile; boosts still reset (ADR-0012).
- Mortgaged tiles stay NOT seizable - kept from ADR-0011, now as a
  deliberate mechanic: mortgaging is the takeover shield, self-priced by
  the lost rent income and the 10% redemption interest. (Decided 2026-07
  over the carry-the-mortgage-with-reduced-compensation alternative.)
- Prices unchanged: cost = `price * rules.expropriation / 100` (base mod
  200), compensation = `min(price, cost)`; both were always based on the
  bare tile price, so they apply as-is with or without buildings. Market
  events (ADR-0021) may scale the cost. Net flows: the old owner gets
  rent + `min(price, cost)` + half-cost per liquidated level; the taker
  pays rent + cost; the bank keeps the premium; the pools regain units.
- "Once per landing" is now structural (one landing, one tile); the old
  "the 2x cost is the natural brake" note becomes moot.

## Consequences
- New rejection variant (e.g. `NotOnTile`) for off-tile attempts;
  existing expropriation tests are updated, new ones cover building
  liquidation and pool returns.
- `Event::Expropriated` grows liquidation detail - a wire change; all
  three clients move the takeover button onto the landed tile and show
  it only there.
- `bot::decide`: consider takeover right after landing (seize when it
  completes an own group or breaks a rival's, cash permitting).
- VP interplay (ADR-0020) is the payoff: stripping a conglomerate or
  breaking a full group removes the defender's points on the spot.
