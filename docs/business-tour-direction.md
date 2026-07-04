# Design direction: fast, dynamic play (Business Tour, not Monopoly)

Status: direction note (not yet implemented). Records the target design and
the gap from today's engine, so changes can be planned deliberately.

## Goal

Parcello's vision is Business-Tour-style play: **short, nervous, tactical
games**, not the slow accumulation of Monopoly. This note lists the
differences that move the game toward fast/dynamic, and how each maps onto
Parcello's architecture.

**Done so far (2026-07):** the default `mods/base` is now a 32-tile fast
board (9x9 ring, no Community Chest, two resorts instead of four stations,
slightly less starting cash); the 40-tile Monopoly-like board moved to
`mods/classic`; the clients render any `4*(d-1)` square ring. The rest of
this note is still ahead. The remaining engine mechanics (below) are what
make it genuinely *Business Tour* rather than "short Monopoly".

Naming caution (commercial plans, see the Steam note): game *mechanics* are
not protectable, but Business Tour's specific names ("Lost Island", "World
Championships", "World Tour") and Monopoly's ("Community Chest") are trade
dress. Parcello already uses original tile names; give these mechanics
**original names too** rather than copying either game's wording.

## Difference map

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

## Suggested path

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
