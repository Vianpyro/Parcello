# Design direction: fast, dynamic play (Business Tour, not Monopoly)

Status: direction note. The v2 ruleset is now fully decided (ADRs
0017-0024, all accepted, 2026-07) - see "V2 ruleset" below for the
summary and build order. The code still implements v1 until those
chantiers land.

## Goal

Parcello's vision is Business-Tour-style play: **short, nervous, tactical
games**, not the slow accumulation of Monopoly. This note lists the
differences that move the game toward fast/dynamic, and how each maps onto
Parcello's architecture.

**Done so far (2026-07):** the default `mods/base` is now a 32-tile fast
board (9x9 ring, no Community Chest, two resorts instead of four stations,
slightly less starting cash); the 40-tile Monopoly-like board moved to
`mods/classic`; the clients render any `4*(d-1)` square ring. V2 build
order step 1 also landed: the blitz clock (12 s turns, 45 s personal time
bank, ADR-0023) and landing-only takeover legality (ADR-0022, the
building-liquidation half still waits on step 2's pools). The rest of
this note is still ahead. The remaining engine mechanics (below) are what
make it genuinely *Business Tour* rather than "short Monopoly".

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
2. ADR-0019 pools + the building-liquidation half of ADR-0022 (improved
   tiles become seizable once buildings can return to shared pools; they
   share the pool accounting).
3. ADR-0021 market forecast (independent).
4. ADR-0018 sealed bids (before points: points measure ownership).
5. ADR-0020 victory points + pool-exhaustion end.
6. ADR-0017 velocity deck + ADR-0024 jail, together (jail is only
   redesigned once the dice are gone). The big one - bot plus most of
   the movement tests - kept last so it never blocks the rest.
   `mods/classic` is removed here.

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
| Jail | jail tile, fine, doubles, cards | "blocked several turns" island | keep the mechanic, **rename** (mod cosmetic); tuning turn count is small **engine** |
| Win condition | last player standing + richest at time limit + control N full groups | also: own all resorts, own a whole side | mostly DONE (time-limit wealth win ADR-0010, domination win ADR-0013); resorts need a string rule, a "side" needs ring geometry - both deferred |
| Time-boxed game | `--game-timeout`: richest by net worth wins at the buzzer | 15/30 min presets, host-chosen | DONE (ADR-0010); host-chosen per-room duration is a follow-up |
| Expropriation | `rules.expropriation`: seize a rival's unimproved property at a premium (owner compensated) | tune cost / allow improved targets | DONE (ADR-0011) |
| Rent multiplier boost | `rules.rent_boost`: pay to raise an owned tile's rent +50%/step, cap 3 | theme it ("championships"), tie to a board event | DONE (ADR-0012) |
| Free-destination move | `MoveTo`/`MoveBy` cards only | "world tour": choose your next landing | **engine** (a choose-destination phase/command) |
| Auctions on decline | on by default | keep - it sustains momentum | already implemented (`rules.auction_on_decline`) |

## Suggested path (v1 note, absorbed by the build order above)

1. **Fast board as the default (DONE).** `mods/base` is now the 32-tile
   fast board and the clients render it as a proper 9x9 ring; `mods/classic`
   keeps the long game. This is the shortest-game lever that needed no new
   engine mechanics.
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
