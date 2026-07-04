# ADR-0011: expropriation (seize a rival's property)

Status: accepted

## Context
The fast/dynamic goal (`docs/business-tour-direction.md`) wants power to
shift quickly. Business Tour lets a player take a city from an opponent.
Monopoly has no such move; trades are the only way ownership changes hands
between players, and they require consent.

## Decision
A new command `Expropriate { tile }` lets the acting player seize a rival's
**unimproved, unmortgaged** property, on their own turn (AwaitRoll or
AwaitEnd; never during an auction - the cash-solvency invariant). It is
gated by `rules.expropriation` (a cost percent; 0 = disabled, default off;
the base fast mod sets 200).

- Cost = `price * pct/100` (e.g. 200 -> 2x price), paid by the seizer.
- The former owner is compensated `min(price, cost)` - at pct >= 100 that is
  the face price, so they recover their investment; the bank keeps the
  premium. This makes it a **forced buyout at a premium**, not theft:
  aggressive enough to break a monopoly, but the victim is not robbed
  (softens frustration, an explicit priority).
- The tile transfers clean: its rent boosts reset (ADR-0012). Houses are
  impossible on it by the precondition.
- Rejections (`ExpropriationDisabled`, `NotExpropriable`,
  `InsufficientFunds`) never mutate (ADR-0001).

Not a per-turn-limited action: the 2x cost is the natural brake. Add a
limit only if playtesting shows runaway seizing.

## Consequences
- New `CommandError` variants and `Event::Expropriated { player, from,
  tile, cost }`; all three clients render it.
- Ownership can now change on a player's own turn without a trade; nothing
  else depended on "ownership only changes via trade/bankruptcy/purchase",
  but keep that in mind.
- Improved/mortgaged properties are protected, bounding the swing.
