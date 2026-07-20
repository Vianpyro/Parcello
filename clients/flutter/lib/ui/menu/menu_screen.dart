/// Step 2 of the client: the main menu.
library;

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../lan_discovery.dart';
import '../../server_manager.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../language_button.dart';
import 'geometry.dart';
import 'menu_tile.dart';
import 'private_table_card.dart';
import 'rules_screen.dart';

/// Step 2 (connected): a grid of large action cards - create or join a
/// private game, browse LAN games / run a server (desktop only), or read the
/// rules. Business-Tour-style tiles, not a form.
class MenuScreen extends StatefulWidget {
  final GameSession s;
  const MenuScreen({super.key, required this.s});

  @override
  State<MenuScreen> createState() => _MenuScreenState();
}

class _MenuScreenState extends State<MenuScreen> {
  void _push(Widget screen) => Navigator.push(
      context, MaterialPageRoute<void>(builder: (_) => screen));

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    // LAN discovery and local server management have no browser equivalent
    // (no raw sockets, no process spawn in a sandbox) - drop those tiles on
    // the web build rather than shipping dead ends.
    final tiles = <Widget>[
      PrivateTableCard(s: s),
      // Watch a running game without playing (ADR-0035): the server picks
      // the fullest human table, else its bots showcase. Doubles as the
      // "see how a game flows" half of the onboarding.
      MenuTile(
          icon: Icons.visibility_outlined,
          title: t.menuWatchTitle,
          subtitle: t.menuWatchSubtitle,
          onTap: s.spectateGame),
      if (!kIsWeb)
        MenuTile(
            icon: Icons.wifi_find_outlined,
            title: t.menuLanTitle,
            subtitle: t.menuLanSubtitle,
            onTap: () => _push(LanBrowser(session: s))),
      if (!kIsWeb)
        MenuTile(
            icon: Icons.dns_outlined,
            title: t.menuServerTitle,
            subtitle: t.menuServerSubtitle,
            onTap: () => _push(const ServerManager())),
      MenuTile(
          icon: Icons.menu_book_outlined,
          title: t.rulesTitle,
          subtitle: t.menuRulesSubtitle,
          onTap: () => _push(const RulesScreen())),
    ];
    return Scaffold(
      appBar: AppBar(
        // Wordmark in Fraunces (display face, visual-identity.md).
        title: Text(t.appTitle,
            style: const TextStyle(
                fontFamily: 'Fraunces',
                fontWeight: FontWeight.w700,
                color: Pc.gold)),
        backgroundColor: Pc.surface2,
        actions: [
          LanguageButton(s: s),
          TextButton.icon(
            onPressed: s.disconnectFromServer,
            icon: const Icon(Icons.logout, size: 18),
            label: Text(t.disconnect),
          ),
        ],
      ),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(Pc.s24),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 680),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                // Grouped so D-pad / arrow keys traverse the tiles directionally
                // (controller + Steam Deck navigation).
                FocusTraversalGroup(
                  policy: ReadingOrderTraversalPolicy(),
                  child: Wrap(
                    spacing: menuGap,
                    runSpacing: menuGap,
                    alignment: WrapAlignment.center,
                    children: tiles,
                  ),
                ),
                const SizedBox(height: Pc.s16),
                Text(s.loginMessage,
                    textAlign: TextAlign.center,
                    style: const TextStyle(color: Pc.oxblood)),
                // Re-arm the first-game coach marks (they self-dismiss
                // forever otherwise).
                TextButton(
                  onPressed: s.resetHints,
                  child: Text(t.menuReplayTips,
                      style:
                          const TextStyle(fontSize: 12, color: Pc.textMuted)),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
