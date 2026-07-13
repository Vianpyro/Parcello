# ADR-0031: a bankruptcy releases the estate to the bank

Status: accepted

## Context
Until now, a debt-driven bankruptcy transferred the debtor's whole portfolio
to the creditor (`apply/cash.rs`, `bankrupt()`), keeping mortgages as-is; only
`Resign` released the estate to the bank. That is the Monopoly rule, and it is
the single largest snowball in the game:

- **It is not a decision, it is a coincidence.** The creditor is whoever
  happened to own the tile the debtor landed on. They pay nothing, choose
  nothing, and receive an entire estate.
- **It short-circuits the race.** Under ADR-0020 a complete colour group is 3
  VP and a conglomerate-level tile is 2. Inheriting a portfolio can hand a
  player several complete groups - and the game - in one landing they did not
  plan. It can also trigger the domination win (ADR-0013) outright.
- **It contradicts the direction.** `docs/business-tour-direction.md` is
  explicit that Parcello is about short, nervous, tactical games driven by
  decisions, not by accumulation. A windfall estate is pure accumulation, and
  it arrives at the exact moment the board should be reopening.
- **It starves the game's core loop.** Every acquisition in Parcello goes
  through a sealed-bid auction (ADR-0018) - that is where the decisions live.
  A bankruptcy is the largest block of property that will ever change hands in
  a match, and routing it around the auction entirely was a waste of the best
  mechanic in the game.

Raised by the owner after a full playtest (2026-07): "toutes les proprietes
devraient seulement etre liberees et requerir a nouveau un achat".

## Decision
- On bankruptcy, **every tile the debtor owned returns to the bank**: unowned,
  unmortgaged, no houses, no boosts. The shared building pools get their units
  back as before (ADR-0019, a pure release). `Event::PropertyTransferred` is
  emitted per tile with `to: None`.
- Bankruptcy and `Resign` are now the *same* estate path. A player leaving the
  game frees the board; they never hand it to somebody.
- The creditor still receives the debtor's **residual cash** (the partial
  settlement in `charge()` is unchanged). The debt is still paid as far as it
  can be - the creditor simply gets no windfall on top.
- `Event::PlayerBankrupt { creditor }` therefore now means "who received the
  residual cash", not "who inherited the estate". The field keeps its shape;
  the wire format does not change.
- The freed tiles re-enter the game through the ordinary sealed-bid auction
  (ADR-0018) the next time anyone lands on them. Mortgages die with their
  owner, so a tile always comes back at its full list price - the takeover
  weak point of ADR-0022 is a live-owner property, not an inheritable one.

## Consequences
- No protocol change, no client change: the Flutter client already renders
  `to: None` (the band sweeps back to no owner) because `Resign` always did.
- Bankruptcy stops being a win condition by proxy. The domination win
  (ADR-0013, off by default) and the VP race (ADR-0020) can no longer be
  handed to a player by someone else's collapse.
- Killing a rival is now worth exactly their cash, not their board. Aggressive
  rent play still pays - it just no longer pays *everything*, and the estate it
  frees is contested by the whole table rather than gifted to one seat.
- Late-game boards reopen instead of consolidating, which is the intended
  effect: a 30-minute game should stay contestable to the end.
- `mods/` cannot turn this off; it is not a `RuleParams` scalar. Promote it to
  one (with an ADR) if a "classic inheritance" mod is ever wanted - the
  `BankruptcyResolver` strategy seam is about *liquidation*, not about who ends
  up owning what, so this would need its own knob rather than a new strategy.
