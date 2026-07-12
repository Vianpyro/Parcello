# ADR-0026: property spotlight (the Exposition corner)

Status: accepted (amended 2026-07: `spotlight_duration_turns <= 0` now
means PERMANENT - only the next Exposition landing replaces the spotlight
- and the base mod uses exactly that; the mechanic's off switch is
`spotlight_rent_pct = 0` or simply not placing a Spotlight tile. The
original 8-turn expiry proved fiddly to track in playtests.)

## Context
`free_parking` (one of the four board corners) has always been a pure
no-op: a player lands there and nothing happens beyond ending the turn.
The owner wanted a Business-Tour-style lever instead - landing there
should hype up a tile, the way Business Tour's "World Championship"
temporarily boosts a property. `CLAUDE.md`'s naming caution is explicit
that Business Tour's specific names ("World Championships", "World
Tour") are trade dress, not the underlying mechanic, so this needs an
original name: the corner is renamed "The Exposition" (the 1925 Paris
*Exposition Internationale des Arts Decoratifs* is literally what Art
Deco is named after, reinforcing `docs/visual-identity.md`'s "city of
progress" register) and the mechanic is called the spotlight.

A companion change lands in the same pass: the base board's resort count
goes from 2 to 4 (evenly spaced every 8 tiles, at the old two positions
plus two more), and chance/tax drop from 2 tiles each to 1. That part is
pure `mods/base/data/properties.toml` data - same category as tuning
`events.toml`'s starter pool (ADR-0021) - and needs no ADR of its own;
it is noted here only because it is what freed the tile that became the
new resort at position 20, and is what's now adjacent to the renamed
corner.

## Decision
- New `TileKind::Spotlight` variant, additive alongside the existing
  `TileKind::FreeParking` rather than repurposing it - `FreeParking`
  stays available as a genuine no-op corner for other mods (e.g.
  `mods/highroller`) or future community content. The base mod's corner
  at board position 16 switches from `free_parking` to `spotlight`
  (id `exposition`, name "The Exposition").
- Landing on it draws one property tile uniformly at random from every
  `Property` tile on the board (any group, owned or not, including
  resorts) via the seeded `GameState.rng` (ADR-0002) - never a player
  choice, so this needs no new `TurnPhase`/command, the cheapest
  possible engine surface. Landing again while one is already active
  re-rolls and replaces it unconditionally: every landing is meaningful,
  never a no-op past the first.
- Two new `RuleParams` scalars, `spotlight_rent_pct` and
  `spotlight_duration_turns`, both defaulting to `0` (the mod-level off
  switch, same idiom as `rent_boost`/`expropriation`). There is no
  separate enable flag: a mod that never places a `Spotlight` tile on
  its board simply never triggers the mechanic, exactly how
  `Chance`/`Tax`/`Community` are already "toggled" by board composition
  rather than a rule. The base mod sets `100`/`8` - rent doubles on the
  spotlit tile for the next 8 turns.
- Composes multiplicatively as a third step in the existing rent chain
  (`resolve_landing`'s owned-property branch): base rent ->
  `boosted_rent` (the owner-paid ADR-0012 step) -> `apply_market_multiplier`
  (the ADR-0021 forecast step) -> the new spotlight step.
- State lives in `GameState.spotlight: Option<Spotlight { tile,
  expires_at_turn }>` - a `GameState` field, mirroring `forecast:
  MarketForecast`, **not** a `TileState` field the way ADR-0012's
  `boosts: u8` is. This is the key design call: it means the spotlight
  survives a trade, expropriation, or bankruptcy transfer of the
  spotlit tile with zero new code in any of those paths, since none of
  them touch `GameState.spotlight` (only `TileState.boosts` gets reset
  on transfer today). The spotlight is a fact about the *location* - a
  table-wide event everyone sees - not an owner-purchased upgrade, so it
  should outlast a sale rather than reset with it.
- `Event::SpotlightStarted { tile, rent_pct, duration_turns }` and
  `Event::SpotlightEnded { tile }` (mirrors `MarketEventActivated`/
  `Expired`'s naming). A bumped spotlight always emits `SpotlightEnded`
  for the old tile immediately before the new `SpotlightStarted`, even
  in the rare case the reroll lands on the same tile again - two
  back-to-back events is a correct, harmless "extended" outcome, not a
  case worth special-casing away.
- Public in `ClientView` unconditionally, no per-seat masking - the
  entire point is that the table sees the hot tile, same reasoning as
  the public forecast queue.
- Bots ignore the spotlight for now, reusing ADR-0021's exact allowance
  ("worse play, not broken play"); `bot::landing_score`'s wildcard arm
  already covers the new tile kind with no code change.

## Consequences
- Protocol fan-out across every crate plus the Flutter client, the same
  bar as any `Event`/`TileKind` change: `crates/engine` (the variant,
  state, event, view, and the `resolve_landing`/`advance_turn` wiring),
  `crates/mods` (`raw.rs` TOML tag, `registry.rs` rule keys),
  `crates/server` (`clamp_settings` bounds on the two new scalars, a
  `-100` floor rather than `0` because this is a multiplier like the
  forecast's `magnitude_pct`, not a pure cost like `rent_boost`),
  `crates/cli` (`describe()` is an exhaustive match on `Event`, so the
  two new variants are compile-required, not optional), and
  `clients/flutter` (`protocol.dart` model + `RuleParams` fields,
  `main.dart` settings panel + a center-panel spotlight line mirroring
  the existing forecast line).
- Tests added in `crates/engine/tests/engine.rs`: activation and rent
  boost on landing, isolation (only the spotlit tile is affected),
  three-way composition with the ADR-0012 boost and the ADR-0021
  forecast, exact-turn expiry, reroll-replaces-and-emits-both-events,
  persistence across a trade (the explicit behavioral contrast with
  ADR-0012), a no-op degrade when the board has no property tiles at
  all, and same-seed replay determinism of the draw. Plus one
  `crates/mods/tests/merge.rs` test that the `spotlight` TOML tag parses.
- The starter numbers (100%, 8 turns) are deliberately rough, calibrated
  the same way ADR-0021's starter event pool was - a playtest task, not
  an engine concern. The one thing worth watching in early playtests:
  this is a *free*, purely random swing with no player agency or cost,
  unlike the paid ADR-0012 boost - if it turns out to swing games too
  hard, the fix is retuning `spotlight_rent_pct`/`spotlight_duration_turns`
  in `mods/base/data/rules.toml`, not an engine change.
- Victory points are unaffected in code (`GameState::victory_points`'s
  `+= 1` per group-scaled tile owned is unconditional on group size) but
  the achievable ceiling from resorts doubles alongside the board
  relayout (2 -> 4 resorts); deferred to playtest rather than adding a
  speculative new tuning scalar - see the board relayout note above.
