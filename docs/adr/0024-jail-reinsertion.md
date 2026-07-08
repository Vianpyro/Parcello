# ADR-0024: the reinsertion plan - jail escape without dice

Status: accepted

## Context
Escape today is dice-shaped: doubles walk free, else pay the fine,
fine forced on the third failure. ADR-0017 removes dice, so escape must
be redesigned; ENTRY does not change (Go To Jail tile, `GoToJail` card -
neither involves dice). The v2 design replaces luck with a choice made
under the blitz clock (ADR-0023): go clean and transparent, or bribe
the table.

## Decision
Three independent exits; the `jail_fine` scalar and the fine path are
removed.

- **Legal Route.** `ChooseLegalRoute { order }` takes a permutation of
  the full fresh hand (all N velocity values, ADR-0017; the current
  hand is discarded). The player leaves jail immediately and the first
  value plays this same turn, but the whole route is locked and PUBLIC
  (`Player.jail_route`, exposed in every view - transparency is the
  price), and while any of it remains their properties charge no rent
  (visitors play free). Each turn the engine accepts only the
  `PlayMovementCard` matching the queue front - which doubles as the
  canonical action, so the AFK machinery needs nothing new. When the
  route empties, the hand refills normally and that refill counts one
  `hands_cycled` (the single ADR-0017 rule), so ADR-0020 rounds keep
  ticking. Building, trading and mortgaging stay allowed - only rent
  income is frozen.
- **Corruption.** `OfferBribe { amount }` (1..=cash, on the jailed
  player's turn, instead of moving) opens a 5-second simultaneous vote
  among living opponents - the ADR-0018 timed-collection window,
  reused: `VoteOnBribe { accept }`, the server injects the canonical
  reject for silent seats, the engine resolves on the last vote.
  Strictly more than half the living opponents must accept (a
  two-player game needs the single opponent's yes). On success the
  amount is deducted only now, split equally among the living opponents
  (floor division; the remainder stays with the briber), and the player
  exits with a normal hand, live rents, and plays their movement this
  same turn. On failure no money moves, the turn degrades to `AwaitEnd`
  (build/trade still fine), and the next attempt - either path - waits
  for their next turn. Individual votes stay secret; the resolution
  event reveals only the outcome and the tally (anti-dogpiling default;
  playtests may open it up). Trades are frozen during the window - the
  bribe is cash-validated at offer time, same invariant family as
  auctions.
- **Jail card.** `UseJailCard` is untouched: immediate unconditional
  exit, then a normal move the same turn. Deliberately the simplest of
  the three - no freeze, no vote, no interaction with the other two.

A jailed seat's canonical/AFK action is the Legal Route in ascending
order - deterministic, and nobody rots in jail (there is no forced fine
any more and no third-roll rule; `jail_turns` simplifies to a plain
jailed flag).

## Consequences
- Commands +3 (`ChooseLegalRoute`, `OfferBribe`, `VoteOnBribe`), events
  for the route/bribe lifecycle, `jail_route` in views: protocol break;
  the jail UI in all three clients becomes a 12-second two-button
  choice (three with a card).
- `mods/highroller` drops its now-unknown `jail_fine` override.
- Tests: route lock enforcement and rent freeze, vote arithmetic,
  retry-next-turn, card bypass, canonical auto-route,
  bankruptcy-during-route (freeze state must purge cleanly).
- `bot::decide`: route in ascending order by default; bribe when cash
  comfortably exceeds the expected freeze loss; vote accept when the
  per-head payout is material. Rough heuristics, tuned at playtests.
