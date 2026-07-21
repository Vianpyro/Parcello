/// The contextual action buttons in the board's centre: the sealed-bid
/// window, the jail decision, the hand, end turn.
///
/// This panel holds the text fields a player types into. It must NOT be
/// rebuilt per animation frame, and it must not reseed a field that is
/// mid-edit - see `_bidInitTile` below and test/bid_input_test.dart.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_chip.dart';
import '../../design/components/pc_textfield.dart';
import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';

class MaxValueFormatter extends TextInputFormatter {
  final int max;
  const MaxValueFormatter(this.max);

  @override
  TextEditingValue formatEditUpdate(
      TextEditingValue oldValue, TextEditingValue newValue) {
    if (newValue.text.isEmpty) return newValue;
    final v = int.tryParse(newValue.text);
    if (v == null) return oldValue; // non-numeric edit (paired with digitsOnly)
    if (v <= max) return newValue;
    final clamped = '$max';
    return TextEditingValue(
      text: clamped,
      selection: TextSelection.collapsed(offset: clamped.length),
    );
  }
}

class ActionsPanel extends StatefulWidget {
  final GameSession s;
  const ActionsPanel({super.key, required this.s});

  @override
  State<ActionsPanel> createState() => ActionsPanelState();
}

class ActionsPanelState extends State<ActionsPanel> {
  final _bid = TextEditingController();
  final _bribe = TextEditingController();
  /// Tile the bid field's current text was seeded for - reseeding only on
  /// a *new* tile (not every rebuild) is the fix for a real bug: this
  /// widget rebuilds on every notifyListeners() (animation beats, other
  /// seats' bids arriving), and unconditionally resetting `_bid.text` each
  /// time made it impossible to type a bid before it got wiped out from
  /// under you (2026-07 playtest feedback).
  int? _bidInitTile;
  /// Same bug, same fix, for the bribe amount field.
  bool _bribeSeeded = false;
  /// Legal Route order built by tapping cards in sequence rather than
  /// typing them (2026-07 playtest feedback: a free-text field either got
  /// mistyped and silently rejected, or - being pre-filled - never edited
  /// at all). Values, not indices: the hand has no duplicates (ADR-0017).
  final List<int> _routeOrder = [];

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final loc = AppLocalizations.of(context);
    final v = s.view;
    if (v == null || v.finished) return const SizedBox.shrink();
    final t = v.turn;

    // Clear the jail-decision UI state the moment we're not actually in
    // that decision (route chosen, bribe sent and the turn moved on, or
    // simply not our situation) - preserved for as long as we ARE still
    // deciding, across however many unrelated rebuilds happen meanwhile.
    final mySeatIdx = s.seat;
    final myPlayer = mySeatIdx != null ? v.players.elementAtOrNull(mySeatIdx) : null;
    final jailDeciding = t.type == 'await_move' &&
        s.myTurn &&
        myPlayer?.inJail == true &&
        myPlayer?.jailRoute == null;
    if (!jailDeciding) {
      _routeOrder.clear();
      _bribeSeeded = false;
    }

    Widget btn(String label, Map<String, dynamic> cmd, {bool primary = true}) {
      return PcButton(label,
          onPressed: () => s.sendCmd(cmd),
          dense: true,
          variant:
              primary ? PcButtonVariant.primary : PcButtonVariant.secondary);
    }

    final children = <Widget>[];
    // Reset once the window closes so a later auction - even on the same
    // tile - always reseeds fresh instead of showing a stale leftover bid.
    if (t.type != 'blind_auction') _bidInitTile = null;
    // Every living seat may bid at once (ADR-0018), not a single actor:
    // show the overlay whenever we haven't submitted yet, regardless of
    // whose turn it nominally is.
    if (t.type == 'blind_auction') {
      final seat = s.seat;
      if (seat == null ||
          t.bids[seat] != null ||
          v.players[seat].bankrupt) {
        return const SizedBox.shrink();
      }
      // The price right now, not the list price: it IS the floor the engine
      // holds bids to (ADR-0021 amended), and the number printed on the tile.
      final price = marketPrice(s.content!.board[t.tile!], v);
      final cash = v.players[seat].cash;
      final isDiscoverer = v.current == seat;
      // The floor binds every seat (ADR-0018 amended 2026-07): below it,
      // the only legal move is to abstain - don't offer a doomed bid.
      final canBid = cash >= price;
      if (_bidInitTile != t.tile) {
        // Seed at the floor: the lowest bid the engine will accept.
        _bid.text = canBid ? '$price' : '0';
        _bidInitTile = t.tile;
      }
      // Quick raises cap at cash: a bid over your balance would just be
      // rejected, so clamp it to an all-in instead (2026-07 feedback).
      void bumpBid(int pct) {
        final current = int.tryParse(_bid.text) ?? price;
        final bump = (price * pct / 100).round();
        _bid.text = '${(current + bump).clamp(0, cash)}';
      }

      children.addAll([
        Text(
          isDiscoverer
              ? loc.actionSealedBidFloor(s.tileName(t.tile!), price)
              : loc.actionSealedBid(s.tileName(t.tile!)),
          style: PcText.label,
        ),
        // The discoverer's edge (ADR-0018 amended): every winner pays in
        // full, then the bank visibly refunds the discoverer 10%.
        if (isDiscoverer)
          Text(
            loc.actionDiscovererHint,
            style: PcText.whisper,
          ),
        if (!canBid)
          // Below the universal floor (ADR-0018 amended 2026-07) any
          // non-zero bid is a guaranteed rejection: say so and offer the
          // one legal move instead of a doomed input.
          Text(
            loc.actionBidCantAfford(price),
            style: PcText.whisper,
          )
        else ...[
          SizedBox(
            width: 90,
            child: PcTextField(
              controller: _bid,
              keyboardType: TextInputType.number,
              dense: true,
              // Digits only, and never more than the seat can afford - the
              // field itself refuses an over-cash bid as you type (2026-07).
              inputFormatters: [
                FilteringTextInputFormatter.digitsOnly,
                MaxValueFormatter(cash),
              ],
            ),
          ),
          PcButton(
            loc.actionBid,
            dense: true,
            // Clamp at submit too, belt-and-suspenders: the field is
            // already capped, but the wire amount must never exceed cash.
            onPressed: () => s.sendCmd({
              'type': 'submit_blind_bid',
              'amount': (int.tryParse(_bid.text) ?? 0).clamp(0, cash),
            }),
          ),
        ],
        btn(loc.actionAbstain, {'type': 'submit_blind_bid', 'amount': 0},
            primary: !canBid),
        if (canBid) ...[
          // Quick raises as a percent of the list price, so escalating a
          // bid doesn't mean typing out full numbers under the clock.
          // Mutating the controller already repaints the TextField bound
          // to it - no setState needed.
          for (final pct in [10, 25, 50, 100])
            PcButton(loc.actionRaisePct(pct),
                dense: true,
                variant: PcButtonVariant.secondary,
                onPressed: () => bumpBid(pct)),
          // All-in: the highest bid the sealed-bid invariant will accept.
          PcButton(loc.actionMaxBid(cash),
              dense: true,
              variant: PcButtonVariant.secondary,
              onPressed: () => _bid.text = '$cash'),
        ],
      ]);
    } else if (t.type == 'bribe_vote') {
      // Every living opponent may vote at once (ADR-0024), not a single
      // actor: show the overlay to anyone except the briber who hasn't
      // voted yet, regardless of whose turn it nominally is.
      final seat = s.seat;
      if (seat == null ||
          seat == t.briber ||
          t.votes[seat] != null ||
          v.players[seat].bankrupt) {
        return const SizedBox.shrink();
      }
      children.addAll([
        Text(
          loc.actionBribePrompt(s.playerName(t.briber!), t.amount!),
          style: PcText.label,
        ),
        btn(loc.actionAccept, {'type': 'vote_on_bribe', 'accept': true}),
        btn(loc.actionReject, {'type': 'vote_on_bribe', 'accept': false},
            primary: false),
      ]);
    } else if (s.myTurn) {
      final me = v.players[s.seat!];
      switch (t.type) {
        case 'await_move':
          final route = me.jailRoute;
          if (route != null) {
            // Locked Legal Route (ADR-0024): only the front card is legal.
            children.add(MouseRegion(
              onEnter: (_) => s.setHoverTile(
                  (me.position + route.first) % s.content!.board.length),
              onExit: (_) => s.setHoverTile(null),
              child: btn(loc.actionPlayRoute(route.first),
                  {'type': 'play_movement_card', 'value': route.first}),
            ));
          } else if (me.inJail) {
            // Three exits: jail card, Corruption bribe, Legal Route.
            if (me.jailCards > 0) {
              children.add(btn(loc.actionUseJailCard, {'type': 'use_jail_card'},
                  primary: false));
            }
            // A Legal Route is a permutation of the full FRESH hand - every
            // velocity value - not of the cards still in hand: choosing it
            // discards whatever is left and deals a whole new hand (ADR-0024,
            // and the rent freeze for the route's whole length is the price of
            // it). Offering the residual hand here built a command the engine
            // could only reject, which made the Legal Route unusable for anyone
            // not jailed on a fresh hand - i.e. almost everyone, since you
            // reach Go To Jail by playing cards (2026-07 playtest).
            final rules = s.settings?.rules;
            final vMin = rules?.velocityMin ?? 2;
            final vMax = rules?.velocityMax ?? 6;
            final sorted = [for (var v = vMin; v <= vMax; v++) v];
            if (!_bribeSeeded) {
              // No suggested-amount cap (2026-07): the engine allows
              // 1..=cash, so seed the full ceiling and let them dial down.
              _bribe.text = '${me.cash > 0 ? me.cash : 1}';
              _bribeSeeded = true;
            }
            final routeComplete = _routeOrder.length == sorted.length;
            children.addAll([
              Text(loc.actionLegalRouteHint,
                  style: PcText.label),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final value in sorted) _routeChip(value),
                ],
              ),
              Row(mainAxisSize: MainAxisSize.min, children: [
                PcButton(
                  loc.actionChooseRoute,
                  dense: true,
                  variant: PcButtonVariant.secondary,
                  onPressed: routeComplete
                      ? () {
                          s.sendCmd({
                            'type': 'choose_legal_route',
                            'order': _routeOrder,
                          });
                          setState(() => _routeOrder.clear());
                        }
                      : null,
                ),
                if (_routeOrder.isNotEmpty) ...[
                  const SizedBox(width: Pc.s6),
                  PcButton(loc.actionReset,
                      dense: true,
                      variant: PcButtonVariant.quiet,
                      onPressed: () => setState(() => _routeOrder.clear())),
                ],
              ]),
              SizedBox(
                width: 90,
                child: PcTextField(
                  controller: _bribe,
                  keyboardType: TextInputType.number,
                  dense: true,
                ),
              ),
              btn(
                  loc.actionOfferBribe,
                  {
                    'type': 'offer_bribe',
                    'amount': int.tryParse(_bribe.text) ?? 0
                  },
                  primary: false),
            ]);
          } else {
            // Hand of movement cards (ADR-0017): one button per card
            // value; hovering one outlines the destination tile on the
            // board (2026-07 playtest feedback).
            final n = s.content!.board.length;
            for (final value in me.hand) {
              children.add(MouseRegion(
                onEnter: (_) =>
                    s.setHoverTile((me.position + value) % n),
                onExit: (_) => s.setHoverTile(null),
                child:
                    btn('$value', {'type': 'play_movement_card', 'value': value}),
              ));
            }
          }
        case 'await_end':
          children.add(btn(loc.actionEndTurn, {'type': 'end_turn'}));
      }
      children.add(Text(loc.actionTapTilesHint,
          style: PcText.caption.copyWith(color: Pc.textFaint)));
    }
    // Grouped so a controller / Steam Deck traverses the action buttons
    // directionally; the Material buttons are already focus-highlighted and
    // Enter/A-activatable. No autofocus here - this panel rebuilds on every
    // server update, and stealing focus each time would fight the player.
    return FocusTraversalGroup(
      policy: ReadingOrderTraversalPolicy(),
      child: Wrap(
          spacing: 6,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: children),
    );
  }

  /// One tappable movement-card chip for the Legal Route builder: tap to
  /// append it to `_routeOrder`, tap an already-picked one to remove it
  /// again (no need for a full reset just to fix one misclick). Picked
  /// chips show their position in the sequence.
  Widget _routeChip(int value) {
    final pos = _routeOrder.indexOf(value);
    final picked = pos >= 0;
    return PcChip(
      picked ? '$value  #${pos + 1}' : '$value',
      selected: picked,
      onTap: () => setState(() {
        if (picked) {
          _routeOrder.remove(value);
        } else {
          _routeOrder.add(value);
        }
      }),
    );
  }
}
