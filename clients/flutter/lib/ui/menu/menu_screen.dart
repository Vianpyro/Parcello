/// Step 2 of the client: the main menu.
library;

import 'package:flutter/foundation.dart' show kDebugMode, kIsWeb;
import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../l10n/app_localizations.dart';
import '../../lan_discovery.dart';
import '../../server_manager.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../language_button.dart';
import '../reconnect_banner.dart';
import '../version_footer.dart';
import 'geometry.dart';
import '../showcase/showcase_screen.dart';
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
      // Debug-only: the design-system component gallery (not shipped to
      // players). English labels are fine on this dev surface.
      if (kDebugMode)
        MenuTile(
            icon: Icons.palette_outlined,
            title: 'Design Showcase',
            subtitle: 'Component gallery (debug)',
            onTap: () => _push(const ShowcaseScreen())),
    ];
    return Scaffold(
      appBar: AppBar(
        // Wordmark in Fraunces (display face, visual-identity.md).
        title: Text(t.appTitle, style: PcText.wordmark),
        backgroundColor: Pc.surface2,
        actions: [
          LanguageButton(s: s),
          PcButton(
            t.disconnect,
            icon: Icons.logout,
            onPressed: s.disconnectFromServer,
            variant: PcButtonVariant.quiet,
            wide: false,
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
                // Socket down / credential spent (ADR-0037). Nothing in the
                // normal case; the menu keeps working while it reconnects.
                ReconnectBanner(s: s),
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
                PcButton(
                  t.menuReplayTips,
                  onPressed: s.resetHints,
                  variant: PcButtonVariant.quiet,
                  wide: false,
                ),
                const SizedBox(height: Pc.s8),
                const VersionFooter(),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
