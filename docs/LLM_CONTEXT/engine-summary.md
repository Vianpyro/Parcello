# Engine summary (`crates/engine`)

Sources of truth: crates/engine sources, ADR-0001/0002/0017-0024/0026/
0029/0031, docs/domain-model.md, docs/INVARIANTS.md sections E*.

## Contract

Pure, synchronous, deterministic. Deps: serde + thiserror only. No I/O,
async, rand, or clock - randomness is one SplitMix64 stream inside
`GameState.rng`, seeded at `new_game`.

```rust
Engine::apply(&state, &PlayerCommand) -> Result<(GameState, Vec<Event>), CommandError>
```

On `Err`, the input state is untouched (validate first, then mutate a
clone). Therefore `(initial players, seed, ordered accepted commands)`
replays bit-identically - guarded by `same_seed_produces_identical_games`
and the fuzzer. `Engine::finish_on_time` is the ONE pure out-of-log step
(time-boxed games, ADR-0010).

## File map

- `lib.rs` - Engine construction; injects Strategy impls
  (`RentCalculator`, `BankruptcyResolver` as `Box<dyn>`); `content()`.
- `apply.rs` + `apply/` - the command pipeline; `Exec` methods split by
  domain (movement, jail, trade, auction, estate, landing, cash, turn),
  all `pub(super)` - the pipeline is the only entry.
- `state.rs` - `GameState`, `Player`, `TileState`, `TurnPhase`
  (incl. `BlindAuction`, `BribeVote`), `TradeOffer`; market types in
  `state/market.rs` (re-exported).
- `content.rs` - `GameContent`, `RuleParams`, `TileKind`, rent models;
  `group_tiles` is a lazy iterator on purpose.
- `tuning.rs` - fixed game-policy numbers NOT mod-configurable (VP
  weights, mortgage %, the 10% discoverer rebate). Promoting one to
  `RuleParams` requires an ADR.
- `view.rs` - `ClientView::for_seat` (masks others' pending bids/votes,
  filters trades to the two parties), `for_spectator` (no trades, all
  pending masked), `of` (omniscient - tooling only, never sent).
- `event.rs` - the event vocabulary; events ARE the replay + animation
  feed.
- `error.rs` - `CommandError`, serialized with tag "code".
- `bot.rs` - the shared autopilot heuristic `bot::decide(content, view,
  seat, noise)`; pure; used by server bot seats AND the CLI `--bot`
  (ADR-0014). Never bids below the market floor
  (`BID_JITTER_MIN_PCT`=100).

## Invariants you will most likely brush against

Universal bid floor (every non-zero bid >= market_price; 0 abstains);
cash frozen during `BlindAuction` (trades rejected); even build/sell
in Build, SellHouse AND forced liquidation; bankruptcy releases the
estate to the bank (never inheritance); `market_price` is the single
price reference (settlement never re-applies multipliers); victory
check after every accepted command; card chains cap at depth 4.

## Extending

New command/event/rule: follow docs/extension-guides.md recipes 1-4 -
they enumerate the mandatory fan-out (fuzzer generator, wire tests,
CLI, Flutter, bot, ARB files). Borrow pitfall: `Exec::owned_property`
borrows from `content` (not `&self`) so callers can mutate state after
- copy that pattern.
