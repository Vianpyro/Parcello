# Domain model

What every game concept IS, which type carries it, why it exists, and how
it lives and dies. Read this before touching rules; read
`docs/business-tour-direction.md` for why the ruleset is shaped this way
(fast, dynamic, Business-Tour-style - explicitly NOT Monopoly's slow
accumulation), and `docs/INVARIANTS.md` for what must never change.

Sources of truth: the types in `crates/engine/src/state.rs` /
`content.rs` / `view.rs`, and the ADRs cited per concept.

## The two state machines

**Room** (session layer, `server/room.rs::Phase`):
`Lobby -> Active -> Finished`, with `PlayAgain` looping Finished back to
Active for the still-connected seats. The architecture doc's `Starting`
state collapsed to a point because mods resolve at boot/creation
(ADR-0004/0006). Rooms dissolve after 30 min with no connected seat AND
no spectator. Ranked rooms (ADR-0034) skip host powers and auto-start;
showcase rooms (ADR-0035) are born `Active` and replay themselves.

**Turn** (engine, `state.rs::TurnPhase`):

```
AwaitMove --PlayMovementCard--> (landing resolution)
   |                                |-- unowned property --> BlindAuction --resolve--> AwaitEnd
   |                                |-- everything else  --> AwaitEnd
   |-- jailed: ChooseLegalRoute / OfferBribe --> BribeVote --> AwaitEnd (or stay jailed)
   |           UseJailCard --> AwaitMove (immediate exit, then play a card)
AwaitEnd --EndTurn--> next seat's AwaitMove
```

`BlindAuction` and `BribeVote` are the two *simultaneous multi-seat*
phases: every living seat acts once, `acting_seat()` is `None`, and each
has its own parallel server timer (12s / 5s) that injects canonical
abstentions for the silent. Anything asynchronous (trades, resign)
is turn-exempt but rejected during those two windows (cash freeze, E6).

## Concepts

### Player / Seat
`state.rs::Player` (engine: id, name, cash, position, hand, jail state,
`hands_cycled`, `round_bonus_vp`, `bankrupt`) vs `room.rs::Seat`
(session: identity, connection, reconnect token, `is_bot`). The engine
player is replay state; the seat is connection state. Seat index is the
join order and the house tie-break everywhere ("ties to the lowest
seat"). Host = seat 0 (plain rooms only). Bots are seats with `tx: None`
and a synthetic `bot:N` identity that the room task plays (ADR-0014);
they yield their seat to a joining human.

### Hand / velocity deck (ADR-0017)
Movement is a public per-player hand of card values
`velocity_min..=velocity_max`, played via `PlayMovementCard`; no dice
exist anywhere. The hand refills to the full range THE INSTANT it
empties, and that refill ticks `hands_cycled` - the round metronome.
Choosing your speed is the core skill hook; the public hand makes the
next few turns partially readable, which the trading and auction
metagames feed on.

### Round & round bonus (ADR-0020)
The round number is the MINIMUM `hands_cycled` across surviving players.
When it rises, the strictly-richest player banks a permanent +2 VP
(`round_bonus_vp`) - an early economic lead leaves a mark even if the
cash later evaporates. Stored, never reversible; every other VP source
mirrors the current board.

### Property / TileState
`content.rs::PropertyDef` (immutable per-mod definition: price, group,
rents, rent model) vs `state.rs::TileState` (owner, houses, mortgaged,
boosts). Rent models: `houses` (full-group x2 unimproved; a singleton
group counts as full) and `group_scaled` (station-like; rejects Build).
`content::group_tiles` is a lazy iterator on purpose - group walks
happen on every landing.

### Sealed-bid auction (ADR-0018, amended twice)
Every landing on an unowned property opens a 12s `BlindAuction`. Every
living seat bids once, secretly; 0 abstains; every non-zero bid must
meet the market price (universal floor, 2026-07); the discoverer holds
an implicit floor bid when silent and solvent; ties favour discoverer
then lowest seat; all-zero leaves the tile unsold. Winner pays IN FULL,
then a winning discoverer is visibly rebated 10% (`DiscovererRefunded`)
- the rebate replaced an invisible discount on purpose: rewards the
table cannot see do not motivate anyone.

### Trade (ADR-0007)
Asynchronous `TradeOffer`s (cash + house-free-group tiles), any solvent
player, any time except during BlindAuction/BribeVote; max 4 open per
proposer; re-validated at acceptance (stale offers reject without
mutation); purged on bankruptcy; PRIVATE to the two parties in views and
event feeds.

### Mortgage (+ the buyout weak point, ADR-0022 amended)
Price/2 out, +10% floored to redeem; house-free group required;
mortgaged tiles pay no rent but count for ownership. A rival's mortgaged
tile is NOT takeover-proof: landing on it buys it at flat mortgage value
- the mortgage is the cheap-buyout weak point, not a shield.

### Jail (entry unchanged, exits redesigned - ADR-0024)
Entry: Go To Jail tile/card. Exits, chosen under the blitz clock:
- **Legal Route** (`ChooseLegalRoute`): lock a public permutation of a
  FULL FRESH hand (whatever was in hand is DISCARDED first - a client
  offering the old cards builds a command the engine rejects); the first
  card plays immediately and un-jails; only the route's front card is
  legal each turn; while any of it remains, the holder's tiles charge NO
  rent; the eventual refill ticks `hands_cycled` once.
- **Corruption** (`OfferBribe` 1..=cash -> `BribeVote`): strictly more
  than half of living opponents must accept; on success the amount
  splits by floor division (remainder stays with the briber) and the
  briber exits clean; on failure no cash moves, turn ends, retry later.
- **Jail card** (`UseJailCard`): a per-player COUNT (not a tradeable
  object; cards never leave the cyclic deck), immediate exit.
The jailed seat's canonical/AFK action is the ascending Legal Route.

### Bank & shared building pools (ADR-0019)
There is no bank actor; "the bank" is the implicit counterparty of every
non-player cash motion. Scarcity is real though: subsidiaries and
conglomerates draw from shared pools sized `round(factor*sqrt(players))`
(0 = unlimited). The top build level converts to a conglomerate and
releases subsidiaries. Forced (bankruptcy) liquidation ALWAYS succeeds,
falling back to a one-motion full strip when the pool can't cover a
step-down. Emptying the conglomerate pool is the doom clock (below).

### Market forecast (ADR-0021) & spotlight (ADR-0026)
A seeded, PUBLIC rolling queue of the next 3 scheduled events
(`rent_multiplier`, `acquisition_multiplier`, one-shot `wealth_tax`) -
draws already made, never the generator. Acquisition events move the
PRICE (`market_price`), never the settlement (E8). The Exposition corner
puts a seeded-random property in the spotlight (rent % boost, permanent
until re-rolled in the base mod); spotlight state lives on `GameState`,
NOT `TileState`, so it survives transfer of the spotlit tile - unlike
the ADR-0012 rent boost, which resets on transfer and is ONE-SHOT (the
first boosted rent consumes it, `RentBoostConsumed`).

### Taxes (ADR-0029)
The base mod's only tax is The Audit (`NetWorthTax`), last tile before
Go: a seeded-random 5-25% slice of net worth, heavier brackets linearly
rarer.

### Bankruptcy & resign (ADR-0031)
Partial payment triggers even-aware liquidation (houses, then
auto-mortgages); if still short, the player is out and the ESTATE IS
RELEASED TO THE BANK - every tile unowned/unmortgaged/stripped, to be
re-won through ordinary auctions; the creditor gets only residual cash.
`Resign` takes the same path. Inheritance must never return (E10).

### Victory & defeat (ADR-0010/0013/0020)
Win conditions, in the order a designer should think of them:
1. **Victory points race** (primary, ADR-0020): first to
   `win_victory_points` (base: 20). +3/complete group, +2/conglomerate
   tile, +1/group-scaled tile - all mirroring the CURRENT board - plus
   the stored round bonus. Checked after every accepted command.
2. **Pool exhaustion** ("doom clock"): the Build that empties the
   conglomerate pool ends the game - points win checked first, then
   highest score, ties by net worth then lowest seat.
3. **Richest at the time limit** (ADR-0010): server clock, engine rule
   (`finish_on_time` is pure and NOT a logged command; replay = replay
   commands, then apply it once - the documented replay extension).
4. **Last player standing.**
5. **Domination** (`win_full_groups`, ADR-0013): off in the base mod so
   it cannot short-circuit the race.
Defeat is bankruptcy/resign; eliminated seats keep their placement order
for ranked scoring (ADR-0034: reverse elimination order).

### Rating (ADR-0034)
Per-server Weng-Lin (mu, sigma) keyed to the token `sub`; displayed as
`max(0, 1000 + 40*(mu - 3*sigma))`. Not the handle. Never guests. A
finished ranked game produces one strict best-to-worst placement.

### Spectator (ADR-0035)
A connection watching a room without a seat: spectator view (no trades,
masked pending bids/votes), no timer influence, unique `watch:` routing
key, counts only against room dissolution. The bots showcase is the
guaranteed something-to-watch when the server is empty.

### Replay
`(initial players, seed, ordered accepted commands)` - stored by
`GameHistory` as `game` + `command` rows. Two documented extensions:
`finish_on_time` (applied after the log for time-boxed games) and
nothing else. There is no replay *player* yet; the format is the
contract that makes building one possible.

### Synchronization (ADR-0028/0030, docs/animation-sync.md)
Server truth vs client rendering meet at the animation-ack watermark:
every `Update` carries `seq`; clients ack rendered-through-N; the
animation-sensitive timers wait, bounded by 10s. The client compiles
each Update into a Plan (pure `director.compile`) whose cost is known
before the first frame and must fit the tiered budget. State is never
lost by skipping - only its journey.

## Deliberate simplifications (do NOT "fix" without an ADR)

- No interest when mortgaged tiles change hands.
- Jail cards are counts, never tradeable, never leave the deck rotation.
- Per-game settings are rules + timers only; the mod set is fixed at
  room creation (ADR-0015).
- Bots do not chase victory points yet (economic heuristic only -
  ADR-0020 accepts this; revisit at playtests).
- No admin control plane on community servers (moderation lives in the
  identity provider; docs/deployment.md).
