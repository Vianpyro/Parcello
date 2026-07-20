# Gameplay summary (the v2 ruleset in play terms)

Sources of truth: docs/business-tour-direction.md (intent), README
"Game rules implemented", docs/domain-model.md, ADRs 0017-0024 + 0026/
0029/0031 + amendments. Base numbers are `mods/base`'s; mods override.

## A turn

You hold a public HAND of movement values (velocity deck, no dice).
Play one -> move that many tiles -> resolve the landing -> optional
estate actions (build/sell/boost/mortgage, landing-tile takeover) ->
end turn. Your hand refills the instant it empties; full refills are
the ROUND metronome.

## Landings

- **Unowned property**: a 12s SEALED-BID auction among ALL living
  seats. Every non-zero bid must meet the market price (universal
  floor); 0 abstains; the lander ("discoverer") silently bids the
  price if solvent, wins ties, and gets a visible 10% rebate when they
  win. All-zero = unsold. Cash is frozen during the window.
- **Owned property**: pay rent (full-group x2 unimproved; group-scaled
  for "utilities"; spotlight/boost/market multipliers compose;
  mortgaged pays nothing). A rival's MORTGAGED tile can be bought out
  at flat mortgage value; an unmortgaged one seized at the
  expropriation premium (landing tile, end of turn, if enabled).
- **Chance**: seeded card (chains cap at 4). **Go To Jail**. **The
  Audit** (last tile before Go): seeded 5-25% net-worth tax, heavier
  brackets rarer. **Exposition corner**: spotlights a random property
  (rent boost that survives transfers; re-landing re-rolls).

## Jail (entry classic, exits redesigned)

Choose under the clock: **Legal Route** (lock a public permutation of
a full FRESH hand - old cards discarded; first card plays now and
frees you; your tiles charge NO rent while the route lasts) |
**Corruption** (bribe 1..=cash; >half of living opponents must accept
in a 5s secret vote; success splits the bribe among them) | **jail
card** (a count, not tradeable). AFK canonical: ascending Legal Route.

## Economy levers

Trades (async, private to the two parties, max 4 open, re-validated at
acceptance, frozen during auctions/votes). Mortgage price/2 out, +10%
to redeem. Shared building pools sized ~factor*sqrt(players):
subsidiaries and conglomerates are FINITE; the top build level
converts to a conglomerate. Rent boosts are one-shot traps (+50%/step,
consumed by the first collection). Public market forecast: the next 3
seeded events visible to all (rent/acquisition multipliers move the
posted price, never the settlement; one-shot wealth tax).

## Endings (design order)

1. **VP race** (primary): first to 20. +3/complete group,
   +2/conglomerate tile, +1/utility owned - all mirroring the CURRENT
   board - plus a permanent +2 each round to the strictly-richest.
2. **Pool exhaustion**: the Build that empties the conglomerate pool
   slams the door - highest score wins (a leader may do it on purpose).
3. **Time limit**: richest by net worth at the buzzer (default 60 min).
4. **Last standing.** 5. **Domination** (off in base).

Bankruptcy/resign: even-aware liquidation, then the ESTATE RETURNS TO
THE BANK (no inheritance); creditor gets residual cash only.

## Feel contract (why it plays fast)

12s turns + a 45s personal time bank; disconnected seats auto-play
canonical (never cash-spending) actions after 30s; every table-wide
decision is a simultaneous timed window, not a round-robin. Animation
pacing is budgeted and acked so the server never races the table.
