# ADR-0027: reveal-and-choose card draws

Status: proposed - written up for review before implementation, unlike
prior ADRs in this series which documented a decision made alongside the
build. Flip to "accepted" once the design below is confirmed.

## Context
Chance/Community draws today are a single blind pick (`DeckState::draw`,
`crates/engine/src/apply.rs::draw_card`) - no agency, pure luck. Comparing
Business Tour's board (screenshot review, 2026-07) surfaced two related
but separate ideas: more Chance-like tiles on the board (a placement
question, already fully supported today with zero engine change - just
more `type = "chance"` tiles sharing the one deck), and a genuinely new
mechanic - reveal several cards and choose one - explicitly requested
instead of just adding tiles ("pas pour ajouter des cases chance, sinon
il n'y aura pas la place pour les pioches"): two "piles" positioned on
opposite sides of the board.

## Decision

**No new `TileKind`.** The two "piles" are the existing `TileKind::Chance`
and `TileKind::Community` - `mods/base` already ships a chance deck and
simply never places a Community tile (`CLAUDE.md`: "no Community Chest").
Activating Community elsewhere on the board, positioned roughly opposite
the existing `chance_1` tile, IS the second pile - a companion data-only
board change, same category as ADR-0026's board relayout, not decided by
this ADR. (Sketch, not fixed: converting `summit_road`, position 19 -
close to opposite `chance_1` at position 3 - would work; red drops from 3
to 2 tiles, same as brown/navy already are today. Exact placement at
implementation time.) This directly answers the tile-budget worry: no new
tile kind, no growth in special-tile count beyond activating a slot the
engine already has fully built and unused.

**One new rule scalar**, `card_choice_count: i64`, default `1`. At `1`
this is *exactly* today's blind draw - zero behavior change for any
existing mod or test unless a mod opts in with a higher value.
`GameContent::validate()` gains a check mirroring the existing
`velocity_min`/`max` non-zero idiom: if any `Chance`/`Community` tile is
on the board, `card_choice_count >= 1`.

**Landing dispatch** (`resolve_landing`'s `TileKind::Chance`/`Community`
arms): if `card_choice_count <= 1`, call `draw_card` exactly as today -
this branch is untouched code. Otherwise:
1. Peek the next `min(card_choice_count, deck.order.len())` cards from
   the deck cyclically (see below) - content indices, not full card data,
   mirroring how `TurnPhase::BlindAuction { tile: usize, .. }` already
   exposes a raw index for the client to resolve against `content`.
2. Push `Event::CardChoiceOpened { player, deck: DeckKind, options:
   Vec<usize> }` - fully public (no secrecy need here, unlike sealed-bid
   amounts; every seat sees what's on offer, which is more fun, not less).
3. Set `TurnPhase::CardChoice { deck: DeckKind, depth: u8, options:
   Vec<usize> }`. `depth` has to live in state now, not a local variable -
   `apply_card_effect`'s chain recursion (`resolve_landing(p, depth + 1)`
   for `MoveTo`/`MoveBy`) crosses an `Engine::apply` command boundary for
   the first time here, since the choice arrives as a separate command.
   Easy to get wrong; flagging explicitly.

**New command**, `CommandKind::ChooseCard { index: usize }`: valid only
in `TurnPhase::CardChoice`, `index` must be `< options.len()`. Consumes
`options[index]` from the deck (see peek/consume below), pushes the
*existing* `Event::CardDrawn` unchanged, then calls the *existing*
`apply_card_effect(p, id, effect, depth)` unchanged - the entire
resolution pipeline (money, `MoveTo`/`MoveBy` chains up to
`MAX_CARD_CHAIN_DEPTH`, `GoToJail`, `GetOutOfJail`, `CollectFromEach`/
`PayEach`) is reused verbatim. Only the reveal-and-wait front end is new.

**Deck mechanism** (`DeckState`, `crates/engine/src/state.rs`): cards are
"drawn in shuffled order and recycled without reshuffling" - a fixed
`order: Vec<u16>` walked by a `next` cursor. Peeking N cards is reading
`order[(next+i) % order.len()]` for `i in 0..N`, unchanged. Choosing
offset `i` removes the absolute index `idx = (next+i) % order.len()` from
`order`; if `idx < next` (the peek window wrapped past the end), `next`
decrements by one to keep pointing at the same logical next card,
otherwise `next` is untouched. The unchosen N-1 peeked cards stay exactly
where they were, next in line for a future draw. No reshuffle, no card
ever lost or discarded - the closest possible extension of the existing
"cyclic, nothing leaves rotation" contract to an N-reveal. If the deck has
fewer than `card_choice_count` cards, peek is naturally bounded by
`order.len()` - degrades to "choose 1 of however many exist," never a
hard error (same spirit as `draw_card`'s existing empty-deck no-op path).

**Gating - no new timer primitive needed.** This is the one place this
design is meaningfully *smaller* than ADR-0018/0024: `CardChoice` is
single-actor (only the landing player decides), not a multi-seat window,
so:
- `crates/server/src/room.rs::acting_seat()` (line 437) already falls
  through its wildcard arm to `Some(st.current)` for any `TurnPhase` it
  doesn't special-case - **no code change needed there**. The existing
  per-turn/time-bank AFK clock (`afk_deadline`) applies to this phase for
  free, exactly like `AwaitMove`/`AwaitEnd` today.
- `crates/engine/src/apply.rs::reject_during_auction()` (line 470, today
  matching `BlindAuction | BribeVote`) gains one more arm,
  `CardChoice { .. }`, unifying trade/build/mortgage blocking with the
  existing two window phases - one line.
- `crates/server/src/room.rs::afk_command()` (line 506) **is** an
  exhaustive match on `TurnPhase` and will not compile without a new arm:
  `TurnPhase::CardChoice { .. } => CommandKind::ChooseCard { index: 0 }`
  - canonical = the first revealed option, consistent with every other
    canonical action in this codebase (lowest hand card, ascending Legal
    Route). `same_seed_produces_identical_games`
    (`crates/engine/tests/engine.rs`) mirrors this same mapping per its
    own doc comment and needs the identical new arm.

## Consequences

- Protocol fan-out (new `TurnPhase` variant + `CommandKind` + `Event` +
  one `RuleParams` field is the same bar as any such change): `crates/mods`
  (`registry.rs` rule key), `crates/server` (`clamp_settings` bound on
  `card_choice_count`, the `afk_command` arm above), `crates/cli`
  (`describe()`'s exhaustive `Event` match needs a `CardChoiceOpened` arm;
  a new interactive prompt for `choose <n>`), `clients/flutter`
  (`protocol.dart`'s `TurnPhase` needs `options`/`deck` fields the way it
  already has `bids`/`votes` for the other two window phases; `main.dart`
  needs a choice UI - closest existing precedent is the bid-amount input,
  not a new pattern).
- Tests to add in `crates/engine/tests/engine.rs`: peek/consume cyclic
  correctness including the wrap-around `idx < next` case; graceful
  degrade when the deck has fewer cards than `card_choice_count`;
  `card_choice_count == 1` is byte-for-byte identical to today's blind
  draw (regression guard, cheapest possible proof this is backward
  compatible); full open -> choose -> effect flow, including a chosen
  `MoveTo` re-triggering `resolve_landing` into a *second* pile
  (chain-depth accounting across the command boundary is exactly the trap
  flagged above); trades/build/mortgage rejected while a choice is
  pending (mirrors the existing `trades_are_blocked_during_auctions`
  test); same-seed determinism of what gets peeked. Plus a
  `crates/mods/tests/merge.rs` case for the new rule key and a wire-format
  test for `ChooseCard`/`CardChoiceOpened`.
- `card_choice_count` is one scalar shared by both Chance and Community -
  no per-deck override. Nothing today motivates asymmetry, and it keeps
  `RuleParams` smaller; each deck still gets its own flavor of cards via
  `cards.toml`, only the reveal count is shared.
- Numbers (how many cards to reveal, e.g. 3) are starter/playtest values
  like every other tunable in this codebase, not decided here.
