# Design direction: fast, dynamic play (Business Tour, not Monopoly)

Status: direction note, now historical for the most part. The v2
ruleset (ADRs 0017-0024, all accepted, 2026-07) is fully **built** - all
six build-order steps below are done, `mods/classic` is gone, and the
code implements v2 end to end. This note stays as the design record and
the difference-map history; see the ADRs themselves for the accepted
decisions.

## Goal

Parcello's vision is Business-Tour-style play: **short, nervous, tactical
games**, not the slow accumulation of Monopoly. This note lists the
differences that move the game toward fast/dynamic, and how each maps onto
Parcello's architecture.

**Done (2026-07):** the default `mods/base` is a 32-tile fast board (9x9
ring, no Community Chest, two resorts instead of four stations, slightly
less starting cash); the clients render any `4*(d-1)` square ring. All
six V2 build-order steps landed: the blitz clock (12 s turns, 45 s
personal time bank, ADR-0023), landing-only takeover legality and
improved-tile liquidation (ADR-0022), shared building pools (ADR-0019),
the public market forecast (ADR-0021), sealed-bid auctions (ADR-0018),
victory points + the pool-exhaustion doom clock (ADR-0020), and finally
the velocity deck + jail rework (ADR-0017/0024) - the 40-tile
`mods/classic` board, the only content using dice-scaled rent, was
removed in that last step (git history keeps it). The build order is
complete; what remains is playtesting and tuning, not new mechanics.

Naming caution (commercial plans, see the Steam note): game *mechanics* are
not protectable, but Business Tour's specific names ("Lost Island", "World
Championships", "World Tour") and Monopoly's ("Community Chest") are trade
dress. Parcello already uses original tile names; give these mechanics
**original names too** rather than copying either game's wording.

## V2 ruleset (decided 2026-07)

The full Business-Tour-style redesign is specified in ADRs 0017-0024
(all accepted). Two cross-cutting calls up front: `mods/classic` leaves
the v2 scope (deleted together with the dice; git history keeps it), so
there is exactly one movement system and one special-tile type
(resorts); and short simultaneous per-seat decisions become a
first-class pattern.

| Mechanic | v2 decision | ADR |
| --- | --- | --- |
| Movement | velocity deck: public hand of `velocity_min..=velocity_max`, no dice, no doubles | 0017 |
| Acquisition | 5 s sealed-bid auction on every landing; discoverer floor = list price; 10% discount only on contested wins | 0018 |
| Building stock | shared pools sized `round(factor * sqrt(players))`: subsidiaries (lower levels) + conglomerates (top level) | 0019 |
| Victory | race to 20 reversible VP (+3/group, +2/conglomerate, +1/resort, +2/round to the cash leader - sticky); pool exhaustion ends the game; domination off | 0020 |
| Events | public 3-slot market forecast; temporary global modifiers, data-calibrated | 0021 |
| Takeover | on the landing tile only, after rent; improved tiles seizable (buildings liquidate to the pools); mortgaged tiles are the shield | 0022 |
| Tempo | 12 s turns + one-shot 45 s personal time bank (server defaults) | 0023 |
| Jail | Legal Route (public locked moves, rents frozen) / Corruption (bribe + 5 s majority vote) / jail card unchanged | 0024 |

Build order (each step keeps `cargo test --workspace --locked` green
and updates web + CLI + Flutter + bot together; every step is a
protocol break, so version accordingly):

1. **DONE (2026-07).** ADR-0023 server defaults (12 s turns + a 45 s
   personal time bank, never refilled) + ADR-0022 landing-only legality
   (`AwaitEnd`, tile == current position) - both ran on the current
   engine, no pools needed yet.
2. **DONE (2026-07).** ADR-0019 pools (subsidiaries/conglomerates, sized
   `round(factor * sqrt(players))`, base mod 6/3) + the building-liquidation
   half of ADR-0022 (improved tiles are seizable, buildings liquidate at
   half cost to the former owner and return to the shared pools). Forced
   (bankruptcy) liquidation stays even-sell but falls back to a one-motion
   full strip when the subsidiary pool can't cover a normal step-down, so
   it can never stall. `mods/classic`/`mods/highroller` deliberately left
   untouched (classic stays unlimited/V1 until its step-6 removal).
3. **DONE (2026-07).** ADR-0021 market forecast: `data/events.toml` (a
   `[forecast] gap_turns` scalar plus a pool of `[[event]]` defs), a
   seeded rolling queue of 3 scheduled events plus one active effect,
   ticked every turn transition. Three effects: `rent_multiplier`
   (composes with the ADR-0012 boost), `acquisition_multiplier` (scales
   takeover cost - sealed-bid pricing from ADR-0018 doesn't exist yet),
   `wealth_tax` (one-shot, every alive player pays a percent of net worth
   through the normal bankruptcy machinery). The base mod ships a starter
   pool (bubble/crash/audit).
4. **DONE (2026-07).** ADR-0018 sealed bids: `TurnPhase::AwaitBuy`/`Auction`
   and `CommandKind::Buy`/`Decline`/`Bid`/`Pass` are gone, replaced by
   `TurnPhase::BlindAuction { tile, bids }` and a single
   `CommandKind::SubmitBlindBid { amount }` (0 = abstain). Every landing on
   an unowned property opens a 5 s window in which every living seat bids
   at once - the first phase where the pipeline's usual single-actor
   assumption doesn't hold; the discoverer gets an implicit list-price
   floor bid if silent and solvent, wins at that floor pay full price, wins
   above it pay 90% (floored); ties favour the discoverer then the lowest
   seat; all-zero effective bids (only possible when a broke discoverer's
   silence has no floor) leave the tile unsold. The server's new
   `bid_deadline` timer is a genuinely separate, parallel primitive from
   the turn clock/time bank (`acting_seat()` returns `None` for the whole
   phase) - the "timed collection window" the cross-cutting note below
   flags as reusable for ADR-0024's corruption vote. `rules.auction_on_decline`
   is gone (there is no plain decline anymore - landing on an affordable
   tile always commits at least the floor).
5. **DONE (2026-07).** ADR-0020 victory points + pool-exhaustion end.
   `RuleParams.win_victory_points` (base mod: 20, `win_full_groups` turned
   off so domination doesn't short-circuit the race). `GameState::
   victory_points`: 3/complete group, 2/conglomerate-level tile,
   1/group-scaled ("resort") tile owned, plus a stored `Player::
   round_bonus_vp` - the only non-reversible term, +2 banked to whoever
   has the strictly highest cash (ties to the lowest seat) each time
   every surviving player has completed a turn. Checked after every
   command like `check_group_win`; reaching the target ends the game
   (`Event::WonByPoints`). Doom clock: if a `Build` empties the shared
   conglomerate pool (ADR-0019) and nobody just crossed the target, the
   game ends immediately - highest score wins, ties by net worth then
   the lowest seat (`Event::WonByPoolExhaustion`); both checks are pure
   game state, no wall clock, ordered so a simultaneous cross is always a
   points win. **Interim decision, revisit at step 6:** the round bonus
   needs "the round" - ADR-0020 defines it as the minimum `hands_cycled`
   (ADR-0017's velocity deck) across surviving players, but ADR-0017
   isn't built yet. Bridged with `Player.hands_cycled: u32`, incremented
   once per completed turn under today's dice movement - "a hand cycled"
   already means "a turn completed" under dice, so only the movement
   mechanism should need to change at step 6, not this field or its
   increment site (`advance_turn`). `ClientView`/`PlayerView` gained
   `victory_points` (computed once server-side, not reimplemented per
   client, learning from `net_worth`'s past triplication) - the first
   `ClientView` methods to need `&GameContent`, so `of`/`for_seat` grew a
   `content` parameter (5 call sites in `server/room.rs`, mechanical).
   `bot::decide` is untouched this step, per the ADR's own allowance.
6. **DONE (2026-07).** ADR-0017 velocity deck + ADR-0024 jail, together
   (jail redesigned once the dice were gone). `Roll`/`PayJailFine` and
   `Event::DiceRolled`/`JailFinePaid` are gone, replaced by
   `PlayMovementCard { value }` (moves from a public `Player.hand`,
   refilled to `velocity_min..=velocity_max` the instant it empties -
   the ADR-0020 round bonus's `hands_cycled` tick moved from
   `advance_turn` to that refill site, exactly the "only the movement
   mechanism changes" prediction from step 5) and the three jail exits:
   `ChooseLegalRoute { order }` (a locked, public permutation of the full
   hand; the first card plays in the same command, each following turn
   only the route's front card is legal, and the route holder's tiles
   charge no rent to visitors until it empties), `OfferBribe { amount }`
   (opens `TurnPhase::BribeVote`, reusing the ADR-0018 timed-collection
   window with its own parallel `vote_deadline` timer rather than a
   shared generic primitive - matching how `game_deadline`/`bid_deadline`
   already coexist; strictly more than half of living opponents must
   accept, floor-division split, remainder stays with the briber), and
   the unchanged `UseJailCard`. `DicePolicy`/`UniformDice` and
   `RentModel::DiceScaled` are deleted outright (engine purity's PRNG,
   ADR-0002, is untouched - only the dice *strategy* went); `mods/classic`
   is removed as its only user. Bot heuristic redesigned (card choice by
   landing score, jail triage: card > bribe > route). The build order is
   now complete - see the ADRs for the accepted specifics.

Cross-cutting: the server gains ONE timed-collection-window primitive
(built for ADR-0018, reused for ADR-0024 votes); the auction
cash-freeze invariant extends to sealed bids and vote windows; every
new phase defines a canonical action so the AFK/blitz machinery and
`same_seed_produces_identical_games` keep working.

The client-side face of all this: `docs/visual-identity.md`.

## Difference map (v1 note, kept for history)

Where rows below conflict with the V2 section, the ADRs win. Notably:
mortgages STAY (they are the takeover shield, ADR-0022, and the
liquidity valve under pool scarcity - the "remove mortgages" idea is
dead); the jail rework became ADR-0024; free-destination moves became
the velocity deck (ADR-0017); the extra win conditions became victory
points (ADR-0020).

Effort key: **mod** = achievable today with a data-only mod (no code);
**rules-flag** = one boolean/scalar in `RuleParams` + a small engine branch;
**engine** = a new mechanic (command/phase/state), warrants an ADR;
**client** = also needs client UI/layout work.

| Aspect | Today (Monopoly-like) | Business-Tour target | Effort in Parcello |
| --- | --- | --- | --- |
| Board length | 40 tiles | ~32 tiles (shorter laps) | DONE - `base` is 32 tiles; clients render any `4*(d-1)` ring |
| Starting cash / salary | 1500 / 200 | tuned for pace | DONE (base 1200/200) + `mods/highroller` for richer |
| Community Chest | second card deck | removed | DONE - dropped from `base` |
| Stations (gares) | 4 group-scaled tiles | removed, or repurposed as "resorts" | DONE - two resorts on `base` |
| Mortgages | full mortgage/redeem flow | removed (slows games) | **rules-flag** (`rules.mortgage`; today the 4 commands are always available - add a disable branch) |
| Jail | jail tile, fine, doubles, cards | "blocked several turns" island | DONE - superseded by the ADR-0024 rework (step 6): Legal Route / Corruption / jail card, no dice |
| Win condition | last player standing + richest at time limit + control N full groups | victory-point race to a target, reversible with the board | DONE - superseded by the ADR-0020 victory-point race (step 5); last-standing and time-limit wealth (ADR-0010) survive as backstops, domination (ADR-0013) is off by default so it doesn't short-circuit the race |
| Time-boxed game | `--game-timeout`: richest by net worth wins at the buzzer | 15/30 min presets, host-chosen | DONE (ADR-0010); host-chosen per-room duration is a follow-up |
| Expropriation | `rules.expropriation`: seize a rival's unimproved property at a premium (owner compensated) | tune cost / allow improved targets | DONE (ADR-0011) |
| Rent multiplier boost | `rules.rent_boost`: pay to raise an owned tile's rent +50%/step, cap 3 | theme it ("championships"), tie to a board event | DONE (ADR-0012) |
| Free-destination move | `MoveTo`/`MoveBy` cards only | "world tour": choose your next landing | **engine** (a choose-destination phase/command) |
| Auctions on decline | on by default | sealed-bid, no plain decline | DONE - superseded by sealed-bid auctions on every landing (ADR-0018, step 4) |

## Suggested path (v1 note, absorbed by the build order above)

1. **Fast board as the default (DONE).** `mods/base` is now the 32-tile
   fast board and the clients render it as a proper 9x9 ring; the
   40-tile long game (`mods/classic`) was kept alongside it until step 6
   removed it with the dice it depended on. This is the shortest-game
   lever that needed no new engine mechanics.
2. **Rule flags for the slow mechanics.** Add `rules.mortgage` (and
   consider gating jail complexity) so a fast mod can turn them off. Small,
   isolated engine branches behind existing seams.
3. **Time limit + wealth win (DONE, ADR-0010).** `--game-timeout` ends the
   game at the buzzer; the richest player by net worth wins. The clients
   show a countdown and a live net-worth ranking. Host-chosen per-room
   durations are the natural next step.
4. **Dynamic mechanics (each its own ADR).** Expropriation (ADR-0011) and
   rent-multiplier boosts (ADR-0012) are DONE and on in the base fast mod.
   Still ahead: free-destination moves and the multi-condition win set
   (own all resorts / all cities of N colours / a whole side). Add them one
   at a time, behind the Strategy/command seams, with tests.

Keep every step data-driven where possible: the mod layer is the right home
for board shape, decks, and rule scalars; only genuinely new *mechanics*
belong in the engine (and then behind an ADR).
