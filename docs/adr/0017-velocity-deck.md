# ADR-0017: velocity deck replaces dice movement

Status: accepted

## Context
Movement is two dice behind the `DicePolicy` strategy (doubles re-roll,
three doubles jail you); `docs/architecture.typ` lists that trait among
the required Strategy seams. The v2 ruleset
(`docs/business-tour-direction.md`, "V2 ruleset") removes luck from
movement entirely: every player holds a public hand of movement values
and spends them - perfect information, no dice. `mods/classic`, the only
dice-flavoured content (`RentModel::DiceScaled` utilities), leaves the
v2 scope.

## Decision
- Two new `RuleParams` scalars: `velocity_min` (engine default 1) and
  `velocity_max` (engine default 5). The hand is every integer in
  `velocity_min..=velocity_max`; its size N = max - min + 1 (default 5,
  the original pitch; a mod may pick e.g. 2..7). `GameContent::validate`
  requires `min >= 1`, `max > min`, `max <= 255` (values travel as u8).
- `Player.hand: Vec<u8>`, dealt full at game start. Public by the same
  doctrine as cash: `PlayerView.hand` shows everyone's remaining cards.
  (The hidden-information rule protects the RNG and deck order, not
  this.)
- New command `PlayMovementCard { value }` replaces `Roll`; `AwaitRoll`
  becomes `AwaitMove`. Playing moves exactly `value` tiles forward,
  removes the value from the hand, then resolves the landing exactly as
  today (Go salary, rent, cards, sealed-bid auction per ADR-0018).
- The hand refills to full the moment it empties, and each refill
  increments a new `Player.hands_cycled: u32`. This single
  "refill = one cycle" rule is the round metronome consumed by ADR-0020,
  and it also covers Legal Route completion (ADR-0024) with no special
  case.
- Everything doubles-related is deleted: `doubles_streak`, the bonus
  re-roll, three-doubles-to-jail, and the doubles escape (escape is
  redesigned in ADR-0024). A turn is exactly one movement card. Jail
  ENTRY is untouched (Go To Jail tile, `CardEffect::GoToJail`), and card
  movement (`MoveTo`/`MoveBy`) is untouched - none of it ever depended
  on dice.
- `DicePolicy` is removed from the engine wiring - a deliberate
  deviation from the Strategy list in `docs/architecture.typ`, recorded
  here. The RNG (ADR-0002) stays: decks and the market forecast
  (ADR-0021) still consume it.
- `RentModel::DiceScaled` is removed, and `mods/classic` is deleted from
  the repo in the same change (git history keeps it; a DLC revival would
  need a movement-mode flag - out of scope until then).

## Consequences
- The largest test churn of the v2 plan: `FixedDice` and every test that
  drives movement through it are rewritten as scripted
  `PlayMovementCard` sequences (strictly simpler - no dice stubbing).
  `same_seed_produces_identical_games` gains the canonical action for
  `AwaitMove`: play the lowest card.
- `bot::decide` chooses a card by scoring the reachable landing tiles
  instead of reacting to a roll.
- Protocol break: `Roll` removed, `PlayMovementCard` added,
  `PlayerView.hand` and `hands_cycled` added. Web client, CLI and
  Flutter update in the same change, as always.
- Choice under time pressure is the game: see the blitz clock,
  ADR-0023.
