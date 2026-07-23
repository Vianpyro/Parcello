/// The one visible trace of ADR-0037's recovery: a strip that says the
/// socket is being re-established, or that the credential finally needs a
/// human.
///
/// Deliberately non-modal and non-blocking, like the coach marks and the
/// post-game survey: recovery is automatic, so the player is being informed,
/// not asked to act. It renders nothing at all in the normal case.
library;

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../session.dart';
import '../tokens.dart';
import '../typography.dart';

class ReconnectBanner extends StatelessWidget {
  final GameSession s;
  const ReconnectBanner({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    // Sign-in expiry is the louder of the two: reconnecting resolves
    // itself, an exhausted credential does not.
    final (message, ink) = switch ((s.signInRequired, s.reconnecting)) {
      (true, _) => (t.signInExpired, Pc.oxblood),
      (_, true) => (t.reconnecting, Pc.gold),
      _ => (null, Pc.gold),
    };
    if (message == null) return const SizedBox.shrink();
    return Align(
      alignment: Alignment.topCenter,
      child: Container(
        margin: const EdgeInsets.only(top: Pc.s8),
        padding: const EdgeInsets.symmetric(
            horizontal: Pc.s12, vertical: Pc.s6),
        decoration: BoxDecoration(
          color: Pc.surface2,
          borderRadius: Pc.radius,
          border: Border.all(color: ink),
        ),
        child: Text(message, style: PcText.caption.copyWith(color: ink)),
      ),
    );
  }
}
