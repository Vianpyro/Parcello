/// First-game coach marks: one contextual hint at a time, shown the first
/// time its situation comes up, dismissed forever with one tap (the menu
/// can replay them). The information arrives when it is needed - the whole
/// point, versus a rules page nobody reads.
library;

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../session.dart';
import '../sfx.dart';
import '../tokens.dart';
import '../typography.dart';

/// The hint the player should see right now, or null. Ordered by urgency:
/// a decision window beats general orientation. Spectators get none - the
/// board itself is their tutorial.
String? activeHintId(GameSession s) {
  if (s.spectating) return null;
  final v = s.view;
  if (v == null) {
    return s.hintSeen('lobby') ? null : 'lobby';
  }
  if (v.turn.type == 'blind_auction' && !s.hintSeen('auction')) {
    return 'auction';
  }
  final seat = s.seat;
  final me = seat == null ? null : v.players.elementAtOrNull(seat);
  if (me != null && me.inJail && s.myTurn && !s.hintSeen('jail')) {
    return 'jail';
  }
  if (s.myTurn && v.turn.type == 'await_move' && !s.hintSeen('hand')) {
    return 'hand';
  }
  if (v.players.any((p) => p.victoryPoints > 0) && !s.hintSeen('vp')) {
    return 'vp';
  }
  return null;
}

String hintText(String id, AppLocalizations t) => switch (id) {
      'lobby' => t.hintLobby,
      'hand' => t.hintHand,
      'auction' => t.hintAuction,
      'jail' => t.hintJail,
      'vp' => t.hintVp,
      _ => '',
    };

/// The hint card itself: quiet parchment-on-dark with the gold accent of
/// the visual identity, never modal, dismissed with one tap.
class CoachMark extends StatelessWidget {
  final String text;
  final VoidCallback onDismiss;
  const CoachMark({super.key, required this.text, required this.onDismiss});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return Card(
      color: Pc.surface2,
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Pc.gold, width: 1),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(Pc.s12, Pc.s8, Pc.s8, Pc.s8),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          const Icon(Icons.tips_and_updates_outlined,
              color: Pc.gold, size: 18),
          const SizedBox(width: Pc.s8),
          Flexible(
            child: Text(text, style: PcText.label),
          ),
          const SizedBox(width: Pc.s4),
          hoverSfx(TextButton(
            onPressed: onDismiss,
            child: Text(t.hintDismiss),
          )),
        ]),
      ),
    );
  }
}
