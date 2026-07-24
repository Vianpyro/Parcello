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
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
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

    // Outline a movement card's destination tile when its button is hovered
    // OR focused - so a keyboard / Steam Deck player (no mouse hover) still
    // sees where a card lands before committing, as they traverse the cards.
    // The wrapper is non-focusable itself (skipTraversal) so it never adds a
    // stop; the button inside stays the focus target.
    Widget previewDest(int dest, Widget child) => Focus(
      canRequestFocus: false,
      skipTraversal: true,
      onFocusChange: (has) => s.setHoverTile(has ? dest : null),
      child: MouseRegion(
        onEnter: (_) => s.setHoverTile(dest),
        onExit: (_) => s.setHoverTile(null),
        child: child,
      ),
    );

    // Clear the jail-decision UI state the moment we're not actually in
    // that decision (route chosen, bribe sent and the turn moved on, or
    // simply not our situation) - preserved for as long as we ARE still
    // deciding, across however many unrelated rebuilds happen meanwhile.
    final mySeatIdx = s.seat;
    final myPlayer = mySeatIdx != null
        ? v.players.elementAtOrNull(mySeatIdx)
        : null;
    final jailDeciding =
        t.type == 'await_move' &&
        s.myTurn &&
        myPlayer?.inJail == true &&
        myPlayer?.jailRoute == null;
    if (!jailDeciding) {
      _routeOrder.clear();
      _bribeSeeded = false;
    }

    Widget btn(String label, Map<String, dynamic> cmd, {bool primary = true}) {
      return PcButton(
        label,
        onPressed: () => s.sendCmd(cmd),
        dense: true,
        variant: primary ? PcButtonVariant.primary : PcButtonVariant.secondary,
      );
    }

    // Five stable zones, always rendered in this order, so the same kind of
    // thing is always found in the same place whatever the phase
    // (SCREEN_ARCHITECTURE: one contextual primary action; no reflow on a state
    // change). A button lands in `primary` or `alternatives` strictly by the
    // variant it ALREADY carries - the zone follows the button, the button is
    // never restyled here.
    final info = <Widget>[]; // what is being decided
    final input = <Widget>[]; // the surface it is composed on
    final primary = <Widget>[]; // the action that commits it
    final alternatives = <Widget>[]; // the other ways out
    final aide = <Widget>[]; // a hint about something else (the tiles)
    // Reset once the window closes so a later auction - even on the same
    // tile - always reseeds fresh instead of showing a stale leftover bid.
    if (t.type != 'blind_auction') _bidInitTile = null;
    // Every living seat may bid at once (ADR-0018), not a single actor:
    // show the overlay whenever we haven't submitted yet, regardless of
    // whose turn it nominally is.
    if (t.type == 'blind_auction') {
      final seat = s.seat;
      if (seat == null || t.bids[seat] != null || v.players[seat].bankrupt) {
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

      // One submit path shared by the Bid button and Enter/Done in the field,
      // so both clamp the wire amount to cash identically (the field is already
      // capped; this is belt-and-suspenders and the single source of truth).
      void submitBid() => s.sendCmd({
        'type': 'submit_blind_bid',
        'amount': (int.tryParse(_bid.text) ?? 0).clamp(0, cash),
      });

      info.addAll([
        Text(
          isDiscoverer
              ? loc.actionSealedBidFloor(s.tileName(t.tile!), price)
              : loc.actionSealedBid(s.tileName(t.tile!)),
          style: PcText.label,
        ),
        // The discoverer's edge (ADR-0018 amended): every winner pays in
        // full, then the bank visibly refunds the discoverer 10%.
        if (isDiscoverer) Text(loc.actionDiscovererHint, style: PcText.whisper),
        if (!canBid)
          // Below the universal floor (ADR-0018 amended 2026-07) any
          // non-zero bid is a guaranteed rejection: say so and offer the
          // one legal move instead of a doomed input.
          Text(loc.actionBidCantAfford(price), style: PcText.whisper),
      ]);
      if (canBid) {
        input.add(
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
              // Enter (or the Deck OSK's Done key) sends the bid without
              // leaving the field for the button - the window is only 12s.
              textInputAction: TextInputAction.done,
              onSubmitted: (_) => submitBid(),
            ),
          ),
        );
        primary.add(PcButton(loc.actionBid, dense: true, onPressed: submitBid));
      }
      // Abstaining IS the move that commits when the floor is out of reach -
      // the button already says so through its variant, so it takes the primary
      // slot exactly then, and the alternatives slot otherwise.
      (canBid ? alternatives : primary).add(
        btn(loc.actionAbstain, {
          'type': 'submit_blind_bid',
          'amount': 0,
        }, primary: !canBid),
      );
      if (canBid) {
        alternatives.addAll([
          // Quick raises as a percent of the list price, so escalating a
          // bid doesn't mean typing out full numbers under the clock.
          // Mutating the controller already repaints the TextField bound
          // to it - no setState needed.
          for (final pct in [10, 25, 50, 100])
            PcButton(
              loc.actionRaisePct(pct),
              dense: true,
              variant: PcButtonVariant.secondary,
              onPressed: () => bumpBid(pct),
            ),
          // All-in: the highest bid the sealed-bid invariant will accept.
          PcButton(
            loc.actionMaxBid(cash),
            dense: true,
            variant: PcButtonVariant.secondary,
            onPressed: () => _bid.text = '$cash',
          ),
        ]);
      }
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
      info.add(
        Text(
          loc.actionBribePrompt(s.playerName(t.briber!), t.amount!),
          style: PcText.label,
        ),
      );
      primary.add(
        btn(loc.actionAccept, {'type': 'vote_on_bribe', 'accept': true}),
      );
      alternatives.add(
        btn(loc.actionReject, {
          'type': 'vote_on_bribe',
          'accept': false,
        }, primary: false),
      );
    } else if (s.myTurn) {
      final me = v.players[s.seat!];
      switch (t.type) {
        case 'await_move':
          final route = me.jailRoute;
          if (route != null) {
            // Locked Legal Route (ADR-0024): only the front card is legal.
            primary.add(
              previewDest(
                (me.position + route.first) % s.content!.board.length,
                btn(loc.actionPlayRoute(route.first), {
                  'type': 'play_movement_card',
                  'value': route.first,
                }),
              ),
            );
          } else if (me.inJail) {
            // Three exits: jail card, Corruption bribe, Legal Route.
            if (me.jailCards > 0) {
              alternatives.add(
                btn(loc.actionUseJailCard, {
                  'type': 'use_jail_card',
                }, primary: false),
              );
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
            // One submit path shared by the Offer button and Enter/Done, read
            // at PRESS time - typing does not rebuild this panel, so a command
            // built at build time would send the last-rendered amount (the
            // seeded ceiling), not the one the player just typed. The bid field
            // already reads at press time; the bribe now matches it.
            void submitBribe() => s.sendCmd({
              'type': 'offer_bribe',
              'amount': int.tryParse(_bribe.text) ?? 0,
            });
            info.add(Text(loc.actionLegalRouteHint, style: PcText.label));
            // The three jail exits are parallel, and each has its own input
            // right beside the button that commits it - so the composers stay
            // one sequence rather than being split field-here / button-there.
            input.addAll([
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [for (final value in sorted) _routeChip(value)],
              ),
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
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
                    PcButton(
                      loc.actionReset,
                      dense: true,
                      variant: PcButtonVariant.quiet,
                      onPressed: () => setState(() => _routeOrder.clear()),
                    ),
                  ],
                ],
              ),
              SizedBox(
                width: 90,
                child: PcTextField(
                  controller: _bribe,
                  keyboardType: TextInputType.number,
                  dense: true,
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) => submitBribe(),
                ),
              ),
              PcButton(
                loc.actionOfferBribe,
                dense: true,
                variant: PcButtonVariant.secondary,
                onPressed: submitBribe,
              ),
            ]);
          } else {
            // Hand of movement cards (ADR-0017): one button per card
            // value; hovering one outlines the destination tile on the
            // board (2026-07 playtest feedback).
            final n = s.content!.board.length;
            for (final value in me.hand) {
              primary.add(
                previewDest(
                  (me.position + value) % n,
                  btn('$value', {'type': 'play_movement_card', 'value': value}),
                ),
              );
            }
          }
        case 'await_end':
          primary.add(btn(loc.actionEndTurn, {'type': 'end_turn'}));
      }
      aide.add(
        Text(
          loc.actionTapTilesHint,
          style: PcText.caption.copyWith(color: Pc.textFaint),
        ),
      );
    }
    // Three reading groups instead of five equal stripes: the decision CONTEXT
    // (what is asked + the surface that composes it), the ACTIONS (the gold
    // commit and its subordinate outline ways-out, read as one cluster), and
    // the tile HINT. Restraint (ART_DIRECTION): whitespace alone groups them -
    // tight inside a group (Pc.s6), a wider beat between (Pc.s8). No rule, no
    // divider, and the panel is not made taller to do it. A peer inside a zone
    // still wraps among its own kind, but a primary never shares a Wrap with a
    // secondary and no text ever shares one with a button.
    Widget? zone(List<Widget> items, Widget Function(List<Widget>) lay) =>
        items.isEmpty ? null : lay(items);
    final groupContext = <Widget>[?zone(info, _stack), ?zone(input, _stack)];
    final groupActions = <Widget>[
      ?zone(primary, _row),
      ?zone(alternatives, _row),
    ];
    final groupHint = <Widget>[?zone(aide, _stack)];
    final groups = <Widget>[
      if (groupContext.isNotEmpty) _group(groupContext),
      if (groupActions.isNotEmpty) _group(groupActions),
      if (groupHint.isNotEmpty) _group(groupHint),
    ];

    // Grouped so a controller / Steam Deck traverses the action buttons
    // directionally; the Material buttons are already focus-highlighted and
    // Enter/A-activatable. No autofocus here - this panel rebuilds on every
    // server update, and stealing focus each time would fight the player.
    return FocusTraversalGroup(
      policy: ReadingOrderTraversalPolicy(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var i = 0; i < groups.length; i++) ...[
            if (i > 0) const SizedBox(height: Pc.s8),
            groups[i],
          ],
        ],
      ),
    );
  }

  /// A reading group: its zones sit tight together (Pc.s6), set apart from the
  /// next group by the caller's wider gap. Whitespace groups; nothing is drawn.
  static Widget _group(List<Widget> zones) => Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      for (var i = 0; i < zones.length; i++) ...[
        if (i > 0) const SizedBox(height: Pc.s6),
        zones[i],
      ],
    ],
  );

  /// A zone of peers laid out side by side, wrapping when the panel is narrow.
  static Widget _row(List<Widget> children) => Wrap(
    spacing: Pc.s6,
    runSpacing: Pc.s6,
    crossAxisAlignment: WrapCrossAlignment.center,
    children: children,
  );

  /// A zone stacked one item per line (text, and the composition surfaces).
  static Widget _stack(List<Widget> children) => Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      for (var i = 0; i < children.length; i++) ...[
        if (i > 0) const SizedBox(height: Pc.s4),
        children[i],
      ],
    ],
  );

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
