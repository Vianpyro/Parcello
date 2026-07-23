/// The market strip under the player bar (game-screen refonte step 1, DDR-0021):
/// the fully-public market state that used to stack as captions inside the
/// board centre - shared building pools (ADR-0019), the market forecast
/// (ADR-0021), and the Exposition spotlight (ADR-0026). No per-seat masking.
///
/// A thin, single-line band: it renders NOTHING when there is no market state
/// to show, so it never steals board height at the 1024x600 floor
/// (SCREEN_ARCHITECTURE / layout_test). The three line builders moved here
/// verbatim from `center_panel.dart` when the centre was reduced to the turn
/// prompt + contextual action panel.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';

class MarketStrip extends StatelessWidget {
  final GameSession s;
  const MarketStrip({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    final parts = [
      ?_poolsLine(t),
      ?_forecastLine(t),
      ?_spotlightLine(t),
    ];
    // Nothing scheduled/active and no pools: the strip takes no vertical space
    // at all rather than a blank band above the board.
    if (parts.isEmpty) return const SizedBox.shrink();
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: Pc.s8, vertical: Pc.s4),
      decoration: const BoxDecoration(
        color: Pc.surface2,
        border: Border(bottom: BorderSide(color: Pc.border)),
      ),
      child: Text(
        parts.join('   •   '),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: PcText.caption,
      ),
    );
  }

  /// Shared building pools (ADR-0019): "the tension only works if everyone
  /// watches the shelf empty." Null when pooling is off entirely.
  String? _poolsLine(AppLocalizations t) {
    final v = s.view;
    if (v == null) return null;
    final subs = v.subsidiariesAvailable;
    final congs = v.conglomeratesAvailable;
    if (subs == null && congs == null) return null;
    return t.poolsLine(
        subs?.toString() ?? t.poolsUnlimited, congs?.toString() ?? t.poolsUnlimited);
  }

  /// Public market forecast (ADR-0021): reveals draws already made, not the
  /// generator. Null when nothing is scheduled or active.
  String? _forecastLine(AppLocalizations t) {
    final v = s.view;
    final c = s.content;
    if (v == null || c == null) return null;
    final f = v.forecast;
    if (f.active == null && f.queue.isEmpty) return null;
    final parts = <String>[];
    if (f.active != null) {
      final a = f.active!;
      final sign = a.magnitudePct > 0 ? '+' : '';
      parts.add(t.forecastActive(
          c.marketEventName(a.eventId), '$sign${a.magnitudePct}', a.endsAtTurn));
    }
    if (f.queue.isNotEmpty) {
      final upcoming = f.queue
          .map((e) =>
              t.forecastUpcomingItem(c.marketEventName(e.eventId), e.startsAtTurn))
          .join(', ');
      parts.add(t.forecastUpcoming(upcoming));
    }
    return parts.join(' | ');
  }

  /// The Exposition corner's spotlight (ADR-0026): fully public, no per-seat
  /// masking. Null when nothing is currently spotlit.
  String? _spotlightLine(AppLocalizations t) {
    final v = s.view;
    final c = s.content;
    final sp = v?.spotlight;
    if (v == null || c == null || sp == null) return null;
    // Prefer the live room rules (host may have tweaked them, ADR-0015);
    // fall back to the content snapshot from join. A permanent spotlight
    // carries u32::MAX as its expiry sentinel - don't print that.
    final pct = s.settings?.rules.spotlightRentPct ?? c.spotlightRentPct;
    final until = sp.expiresAtTurn >= 0xFFFFFFFF
        ? t.spotlightUntilReplaced
        : t.spotlightEndsTurn(sp.expiresAtTurn);
    return t.spotlightLine(c.board[sp.tile].name, pct, until);
  }
}
