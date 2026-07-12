# ADR-0029: progressive net-worth tax tile (The Audit)

Status: accepted

## Context
The board's one tax tile was a flat `Tax { amount: 200 }` - trivial for a
rich player, brutal for a poor one, and (first playtests, 2026-07) simply
boring: a fixed, known number generates no tension. The owner wanted it
replaced with a tax on a random *portion* of the lander's net worth,
where heavier brackets are rarer ("the higher the part the lower the
chance").

## Decision
- New `TileKind::NetWorthTax { min_pct, max_pct }` (TOML `type =
  "net_worth_tax"` with `min_pct`/`max_pct`), additive alongside the flat
  `Tax` which stays for mods that want predictability.
  `GameContent::validate` requires `1 <= min_pct <= max_pct <= 100`.
- Landing draws a bracket from the seeded RNG (`GameState::
  draw_networth_tax_pct`, same state-owns-the-draw convention as the
  spotlight and forecast) with **linearly decreasing weight**: the weight
  of percent `p` is `max_pct - p + 1`, so for the base mod's 5-25% range
  the 5% bracket is 21x more likely than the 25% one. Deterministic,
  replay-safe (ADR-0002).
- The amount is `net_worth * pct / 100` - the same `GameState::net_worth`
  formula the timed-game ranking uses (cash + property equity + houses) -
  charged through the ordinary `TaxPaid`/`charge` machinery, partial-
  payment bankruptcy included. No new event type.
- `mods/base`: `income_tax` (position 31, the last tile before Go)
  becomes `audit` ("The Audit"), `net_worth_tax 5-25%`. The flat-tax tile
  count in the base mod drops to zero; the `tax` type itself remains
  supported.

## Consequences
- Protocol/client fan-out: `TileDef` grows optional `min_pct`/`max_pct`
  on the wire (serde-flat via the mod layer); the Flutter client shows
  "5-25% NW" as the tile meta; the CLI needs nothing (tile names carry).
- Tests: bracket draw is seed-deterministic and always lands on an exact
  configured percent of net worth; validation rejects inverted brackets;
  `crates/mods` parses the new TOML type.
- A late-game landing can now cost hundreds (25% of a big portfolio) -
  intentionally swingy, sized by the same playtest-tunable philosophy as
  every other scalar. The percent range is mod data, not engine policy.
- Position 31 makes the last step before Go a wealth check: the richer
  the lap, the scarier the corner - the exact inversion of the old flat
  tax, which the poor feared and the rich ignored.
