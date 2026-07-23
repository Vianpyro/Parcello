/// The left navigation rail (game-screen refonte, DDR-0021): a thin vertical
/// strip of icon buttons that open real, existing content as sheets - Menu
/// (settings / leave / room code, plus resign when `s.canResign`), Objectives
/// (the win condition and VP scoring), and History (the event feed). No
/// graphical placeholders: every button does something real, reusing the
/// existing widgets. Resign is the single copy of that action (the old
/// SidePanel duplicate is gone) and the only place gated on `canResign`.
///
/// The mockup's fourth entry, Chat, is intentionally OMITTED: it has no backend
/// (DDR-0023 keeps it a deferred placeholder feature, and this refonte adds no
/// server), and a dead button would be exactly the placeholder we avoid.
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_dialog.dart';
import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../common.dart';
import '../side/settings_panel.dart';
import 'event_log.dart';

class NavRail extends StatelessWidget {
  final GameSession s;
  const NavRail({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return Container(
      width: 64,
      padding: const EdgeInsets.symmetric(vertical: Pc.s8),
      decoration: const BoxDecoration(
        color: Pc.surface2,
        border: Border(right: BorderSide(color: Pc.border)),
      ),
      child: FocusTraversalGroup(
        child: Column(
          children: [
            _RailButton(
              icon: Icons.menu,
              label: t.navMenu,
              onTap: () => _menu(context),
            ),
            const SizedBox(height: Pc.s8),
            _RailButton(
              icon: Icons.flag_outlined,
              label: t.navObjectives,
              onTap: () => _objectives(context),
            ),
            const SizedBox(height: Pc.s8),
            _RailButton(
              icon: Icons.history,
              label: t.navHistory,
              onTap: () => _history(context),
            ),
          ],
        ),
      ),
    );
  }

  void _sheet(BuildContext context, Widget child) {
    showModalBottomSheet<void>(
      context: context,
      builder: (ctx) => SafeArea(
        child: Padding(padding: const EdgeInsets.all(Pc.s16), child: child),
      ),
    );
  }

  void _menu(BuildContext context) {
    final t = AppLocalizations.of(context);
    _sheet(
      context,
      Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (s.code != null)
            Row(
              children: [
                Expanded(
                  child: Text(t.sideRoom(s.code!), style: PcText.rowTitle),
                ),
                IconButton(
                  tooltip: t.copyRoomCode,
                  icon: const Icon(Icons.copy, color: Pc.textMuted),
                  onPressed: () => copyCode(context, s.code!),
                ),
              ],
            ),
          if (s.settings != null) ...[
            const SizedBox(height: Pc.s8),
            SettingsPanel(s: s),
          ],
          const SizedBox(height: Pc.s8),
          Row(
            children: [
              Expanded(
                child: PcButton(
                  t.leave,
                  onPressed: () {
                    Navigator.pop(context);
                    s.leaveRoom();
                  },
                  variant: PcButtonVariant.secondary,
                ),
              ),
              // Resign is a GAME action, not a global one: hidden outside an
              // active game, once I hold no seat (spectator, ADR-0035), or once
              // I am already bankrupt - there is nothing left to forfeit.
              if (s.canResign) ...[
                const SizedBox(width: Pc.s8),
                Expanded(
                  child: PcButton(
                    t.resign,
                    onPressed: () => _confirmResign(context),
                    variant: PcButtonVariant.destructive,
                  ),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }

  /// The resign confirm flow (single source of truth): a destructive
  /// `PcDialog`, the confirm sound, then the resign command - unreachable
  /// unless `s.canResign` gated the trigger that called this.
  Future<void> _confirmResign(BuildContext context) async {
    final t = AppLocalizations.of(context);
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => PcDialog(
        title: t.resignConfirmTitle,
        cancelLabel: t.cancel,
        primaryLabel: t.resign,
        destructive: true,
        onPrimary: () {
          sfx.buttonYes();
          Navigator.pop(ctx, true);
        },
      ),
    );
    if (ok == true) {
      if (context.mounted) Navigator.pop(context);
      s.sendCmd({'type': 'resign'});
    }
  }

  void _objectives(BuildContext context) {
    final t = AppLocalizations.of(context);
    final target = s.content?.winVictoryPoints ?? 0;
    final rows = <(String, String)>[
      ('3', t.vpLegendFullGroup),
      ('2', t.vpLegendMaxedTile),
      ('1', t.vpLegendUtilityTile),
      ('+2', t.vpLegendRoundBonus),
    ];
    _sheet(
      context,
      Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            target > 0 ? t.vpLegendHeader(target) : t.navObjectives,
            style: PcText.section,
          ),
          const SizedBox(height: Pc.s8),
          for (final (pts, label) in rows)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Pc.s2),
              child: Row(
                children: [
                  SizedBox(
                    width: 32,
                    child: Text(
                      pts,
                      style: PcText.rowTitle.copyWith(color: Pc.gold),
                    ),
                  ),
                  const SizedBox(width: Pc.s8),
                  Expanded(child: Text(label, style: PcText.body)),
                ],
              ),
            ),
        ],
      ),
    );
  }

  void _history(BuildContext context) {
    _sheet(context, SizedBox(height: 320, child: EventLog(log: s.log)));
  }
}

/// One rail entry: an icon over a small label, keyboard/controller focusable.
class _RailButton extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;
  const _RailButton({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return hoverSfx(
      TextButton(
        onPressed: onTap,
        style: TextButton.styleFrom(
          foregroundColor: Pc.textMuted,
          padding: const EdgeInsets.symmetric(
            vertical: Pc.s6,
            horizontal: Pc.s2,
          ),
          minimumSize: const Size(0, 0),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 22),
            const SizedBox(height: Pc.s2),
            Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: PcText.whisper,
            ),
          ],
        ),
      ),
    );
  }
}
