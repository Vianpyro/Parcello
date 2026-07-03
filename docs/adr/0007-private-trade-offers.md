# ADR-0007: private trade offers via per-seat views

Status: accepted

## Context
V1 made trade offers public: `ClientView` was identical for every seat and
the room broadcast one message to all. Business-Tour-style play expects
negotiations to be private between the two parties.

## Decision
The engine gains `ClientView::for_seat(state, seat)`: the public
projection with `pending_trades` filtered to offers the seat proposed or
received. `ClientView::of` remains as the omniscient view for tests and
replay tooling only - the server always sends `for_seat` views.

The session layer builds one message per seat for `GameStarted`, `Update`,
and the mid-game `Joined` snapshot. Trade lifecycle events
(`TradeProposed`, `TradeAccepted`, `TradeDeclined`, `TradeCancelled`) are
filtered the same way; `TradeDeclined`/`TradeCancelled` gained `from`/`to`
fields (additive, old readers ignore them) so the room can route them.

## Consequences
- The wire shapes are unchanged; only per-seat content differs. Clients
  needed no changes: their trade UI already keyed on "my" offers.
- Third parties still observe the *effects* of an accepted trade (cash and
  ownership are public state); they just never see the offer or its
  lifecycle. This is the intended semantic of a private deal.
- Cash stays public (unchanged from the reference game).
- The per-command broadcast now clones one filtered view per seat (max 6).
  Negligible at human command rates.
