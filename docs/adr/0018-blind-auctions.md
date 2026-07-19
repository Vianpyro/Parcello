# ADR-0018: sealed-bid auctions replace buy/decline

Status: accepted

## Context
Landing on an unowned property offers it at list price (`AwaitBuy`,
`Buy`/`Decline`), with a round-robin open auction on decline
(`TurnPhase::Auction`, `Bid`/`Pass`, gated by `rules.auction_on_decline`).
The v2 ruleset makes every acquisition contested: landing on a free
property opens one 5-second, simultaneous, sealed-bid auction. No
uncontested list-price purchase exists any more.

## Decision
- New `TurnPhase::BlindAuction { tile, bids }` replaces both `AwaitBuy`
  and `Auction`; the commands `Buy`, `Decline`, `Bid`, `Pass` and the
  `auction_on_decline` scalar are removed. New command
  `SubmitBlindBid { amount }`; `amount = 0` means abstain.
- The lander (the "discoverer") holds an implicit floor bid equal to the
  list price, provided their cash covers it; an explicit discoverer bid
  must be >= that floor. Every other living seat may bid any amount up
  to their cash (a broke discoverer has no floor and bids like anyone
  else). Bids are validated against cash at submit time and cash is
  frozen while the phase is open - trades are rejected, extending
  today's auction-solvency invariant - so the winner can always pay.
  **Amended 2026-07 (universal floor):** the floor now binds every
  bidder, not only the discoverer - any non-zero bid below the current
  market price is rejected (`BidBelowFloor`); `0` remains the only way
  to abstain. Playtests on the public server showed the original rule's
  side channel: whenever the discoverer could not cover the price (so
  held no implicit floor bid), a rival could take the tile for 1$ - a
  win that felt like a glitch, not a snipe. The trade-off is accepted
  knowingly: a seat that cannot afford the market price can no longer
  participate at all, and unwanted tiles will stay unsold more often.
  Bankruptcy releasing tiles back to the bank (ADR-0031) leans on those
  re-auctions, so watch pacing in playtests. The engine's bots already
  never bid sub-floor (bot.rs `BID_JITTER_MIN_PCT` = 100), so only
  humans are affected.
- Resolution is pure and happens inside `apply` the moment every living
  seat has a recorded bid: highest amount wins; ties go to the
  discoverer, then the lowest seat (the house convention); if every
  recorded bid is 0 the tile stays unsold - exactly today's
  "no bids = unsold".
- Discoverer rebate (amended 2026-07; superseded the original discount):
  every winner, discoverer included, pays their winning bid **in full**.
  A discoverer that wins is then handed back `settlement * 10 / 100`,
  floored, by the bank, as a separate `DiscovererRefunded` event. Any
  other winner gets nothing back. Market events (ADR-0021) may scale the
  settled price, and the rebate follows what was actually paid.

  Superseded: the discoverer used to pay `amount * 90 / 100` when winning
  strictly above the floor, and full price at the floor. Two reasons to
  change. It was invisible - a discount is a number that never happens on
  screen, so the reward for landing there was never *seen*; paying in full
  and being paid back is two motions the table can watch. And it was
  conditional in a way nobody could hold in their head: the reward
  appeared only after a contest, and vanished if you won at your own
  floor. The rebate is now the discoverer's whole edge on price, and it
  applies whenever they win. The implicit floor bid and the tiebreak above
  are unchanged - those are structural, not rewards: without the implicit
  bid, landing on a tile and staying silent would leave it unsold.
- Secrecy: pending bids are hidden state. `ClientView` masks other
  seats' bids while the phase is open (a seat sees only its own), and
  the acceptance event carries no amount. The resolution event then
  reveals every bid - post-hoc transparency, same doctrine as public
  cash. This is the first `TurnPhase` payload filtered per seat; while
  the phase is open it joins `rng`/deck order on the never-expose list.
- Clock: the engine stays clockless. The server arms a 5-second window
  (like `afk_deadline`) and at expiry injects the canonical
  `SubmitBlindBid { amount: 0 }` for every silent seat - the same
  auto-play machinery as AFK turns. Every bid, injected or not, is an
  ordinary accepted command, so the replay contract (ADR-0001) holds
  verbatim; unlike `finish_on_time` (ADR-0010) no out-of-log step is
  needed.
- Server primitive: this "timed collection window" (arm a deadline,
  collect one submission per seat, inject canonicals at expiry, let the
  engine resolve on the last one) is written once and reused by the
  corruption vote (ADR-0024).

## Consequences
- Protocol break: four commands removed, one added, the phase shape
  changed; the buy dialog in all three clients becomes a 5-second bid
  overlay.
- `bot::decide` needs a sealed-bid heuristic (value the tile, bid within
  cash, abstain when poor).
- Canonical action for the `same_seed` guard and AFK machinery: abstain.
- Auction tests are rewritten; the trade-freeze tests (ADR-0007) extend
  to the new phase.
