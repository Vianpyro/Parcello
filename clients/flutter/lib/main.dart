/// Parcello Flutter client: desktop (Windows/Linux/macOS) and web from one
/// codebase. The server stays the only authority.
library;

import 'dart:async';

import 'package:flutter/foundation.dart'
    show kIsWeb, LicenseRegistry, LicenseEntryWithLineBreaks;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'back_on_escape.dart';
import 'board.dart';
import 'l10n/app_localizations.dart';
import 'motion.dart';
import 'oidc.dart';
import 'overlay.dart';
import 'protocol.dart';
import 'session.dart';
import 'sfx.dart';
import 'stage.dart';
import 'tokens.dart';
import 'lan_discovery.dart';
import 'server_manager.dart';

void main() {
  _registerFontLicenses();
  runApp(ParcelloApp(session: GameSession()));
}

/// Make the bundled OFL font licences discoverable in-app (showLicensePage),
/// as the SIL Open Font License asks when a font is redistributed. The texts
/// ship as assets (see pubspec.yaml); this appends them to Flutter's registry
/// without replacing the framework's own entries.
void _registerFontLicenses() {
  LicenseRegistry.addLicense(() async* {
    for (final family in ['Inter', 'Fraunces', 'SourceSerif4']) {
      final text = await rootBundle.loadString('assets/fonts/$family-OFL.txt');
      yield LicenseEntryWithLineBreaks(['Parcello fonts', family], text);
    }
  });
}

class ParcelloApp extends StatelessWidget {
  final GameSession session;
  const ParcelloApp({super.key, required this.session});

  @override
  Widget build(BuildContext context) {
    // Only a language change rebuilds MaterialApp - deliberately NOT the
    // session's own notifier, which fires on every server update.
    return ValueListenableBuilder<String>(
      valueListenable: session.localeTag,
      builder: (context, tag, _) => _app(tag),
    );
  }

  /// `tag` empty = no override, so Flutter resolves the system locale.
  Widget _app(String tag) {
    return MaterialApp(
      locale: tag.isEmpty ? null : Locale(tag),
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: ThemeData(
        brightness: Brightness.dark,
        // Inter is the body/UI family (docs/visual-identity.md); Fraunces
        // (wordmark) and SourceSerif4 (tile labels) are applied at their
        // specific use sites. Bundled offline - assets/fonts/.
        fontFamily: 'Inter',
        scaffoldBackgroundColor: Pc.bg,
        colorScheme: ColorScheme.fromSeed(
          seedColor: Pc.gold,
          brightness: Brightness.dark,
        ).copyWith(surface: Pc.surface, error: Pc.oxblood),
        // Sharp corners everywhere: no pills, no soft blobs. Art direction, not
        // preference (`docs/visual-identity.md`).
        cardTheme: const CardThemeData(
            shape: RoundedRectangleBorder(borderRadius: Pc.radius)),
        filledButtonTheme: FilledButtonThemeData(
            style: FilledButton.styleFrom(
                shape: const RoundedRectangleBorder(borderRadius: Pc.radius))),
        outlinedButtonTheme: OutlinedButtonThemeData(
            style: OutlinedButton.styleFrom(
                shape: const RoundedRectangleBorder(borderRadius: Pc.radius))),
        dialogTheme: const DialogThemeData(
            shape: RoundedRectangleBorder(borderRadius: Pc.radius)),
      ),
      home: ListenableBuilder(
        listenable: session,
        builder: (context, _) {
          // This builder sits under MaterialApp's Localizations, so it is the
          // earliest place the (context-free) session can be handed its
          // AppLocalizations for the event log - refreshed every frame, set
          // before any server message is processed.
          session.l10n = AppLocalizations.of(context);
          if (session.joined) return GameScreen(s: session);
          if (session.connected) return MenuScreen(s: session);
          return ConnectScreen(s: session);
        },
      ),
    );
  }
}

/// Copies a room code and confirms with a brief snackbar.
void copyCode(BuildContext context, String code) {
  Clipboard.setData(ClipboardData(text: code));
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(
    content: Text(AppLocalizations.of(context).roomCodeCopied(code)),
    duration: const Duration(seconds: 1),
  ));
}

/// A tall, full-width button so every screen ports to touch with minimal
/// change. Primary = filled, secondary = outlined.
Widget wideButton(String label, VoidCallback? onPressed, {bool primary = true}) {
  final style = ButtonStyle(
    minimumSize: WidgetStateProperty.all(const Size.fromHeight(52)),
    textStyle: WidgetStateProperty.all(
        const TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
  );
  return hoverSfx(primary
      ? FilledButton(onPressed: onPressed, style: style, child: Text(label))
      : OutlinedButton(onPressed: onPressed, style: style, child: Text(label)));
}

// -- connect -------------------------------------------------------------------

/// Step 1: connect to a server with an identity. The connection is kept open
/// so the menu (step 2) can create/join without reconnecting.
class ConnectScreen extends StatefulWidget {
  final GameSession s;
  const ConnectScreen({super.key, required this.s});

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _url = TextEditingController(text: 'ws://127.0.0.1:7878/ws');
  final _name = TextEditingController();
  final _token = TextEditingController();
  String? _signedInAs;

  /// OIDC login (ADR-0009): asks for the issuer URL, runs the browser
  /// PKCE flow, and drops the id_token into the token field.
  Future<void> _signIn() async {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    final issuer = TextEditingController(
        text: s.savedIssuer.isEmpty ? 'https://' : s.savedIssuer);
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(t.signIn),
        content: TextField(
          controller: issuer,
          decoration: InputDecoration(
              labelText: t.identityProviderUrl,
              hintText: 'https://auth.example.com'),
        ),
        actions: [
          hoverSfx(TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: Text(t.cancel))),
          hoverSfx(FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: Text(t.openBrowser))),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    try {
      s.saveIssuer(issuer.text.trim());
      final token = await loginWithOidc(issuer.text.trim(), 'parcello');
      setState(() {
        _token.text = token;
        _signedInAs = jwtDisplayName(token) ?? t.account;
      });
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(t.signInFailed(e.toString()))));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          child: Card(
            child: Container(
              width: 380,
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Align(
                      alignment: Alignment.centerRight,
                      child: _LanguageButton(s: s)),
                  Text(t.appTitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                          fontSize: 30,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: 2),
                  Text(t.connectSubtitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                  const SizedBox(height: 16),
                  TextField(
                    controller: _url,
                    decoration: InputDecoration(labelText: t.serverUrl),
                  ),
                  TextField(
                    controller: _name,
                    maxLength: 24,
                    decoration: InputDecoration(labelText: t.displayName),
                  ),
                  const SizedBox(height: 8),
                  wideButton(
                      _signedInAs == null
                          ? t.signInOptional
                          : t.signedInAs(_signedInAs!),
                      _signIn,
                      primary: false),
                  const SizedBox(height: 10),
                  wideButton(t.connect, () {
                    if (_name.text.trim().isEmpty &&
                        _token.text.trim().isEmpty) {
                      return;
                    }
                    s.connect(_url.text.trim(), _name.text.trim(),
                        token: _token.text.trim());
                  }),
                  const SizedBox(height: 8),
                  Text(s.loginMessage,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// -- menu ----------------------------------------------------------------------

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
      _PrivateTableCard(s: s),
      if (!kIsWeb)
        _MenuTile(
            icon: Icons.wifi_find_outlined,
            title: t.menuLanTitle,
            subtitle: t.menuLanSubtitle,
            onTap: () => _push(LanBrowser(session: s))),
      if (!kIsWeb)
        _MenuTile(
            icon: Icons.dns_outlined,
            title: t.menuServerTitle,
            subtitle: t.menuServerSubtitle,
            onTap: () => _push(const ServerManager())),
      _MenuTile(
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
          _LanguageButton(s: s),
          TextButton.icon(
            onPressed: s.disconnectFromServer,
            icon: const Icon(Icons.logout, size: 18),
            label: Text(t.disconnect),
          ),
        ],
      ),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(24),
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
                    spacing: _menuGap,
                    runSpacing: _menuGap,
                    alignment: WrapAlignment.center,
                    children: tiles,
                  ),
                ),
                const SizedBox(height: 16),
                Text(s.loginMessage,
                    textAlign: TextAlign.center,
                    style: const TextStyle(color: Pc.oxblood)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Language names are endonyms - a language is always named in its own
/// language, never translated - so they are data here rather than ARB strings.
/// Keyed by the codes in `AppLocalizations.supportedLocales`; a locale added
/// without an entry falls back to its code rather than disappearing.
const _languageNames = {'en': 'English', 'fr': 'Français'};

/// Picks the UI language: system default, or a forced locale. Available before
/// connecting (the very place someone whose OS is in another language needs it)
/// and from the menu.
class _LanguageButton extends StatelessWidget {
  final GameSession s;
  const _LanguageButton({required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return hoverSfx(PopupMenuButton<String>(
      icon: const Icon(Icons.language, size: 18, color: Pc.textMuted),
      tooltip: t.language,
      initialValue: s.localeTag.value,
      onSelected: s.setLocaleTag,
      itemBuilder: (_) => [
        PopupMenuItem(value: '', child: Text(t.languageSystem)),
        for (final l in AppLocalizations.supportedLocales)
          PopupMenuItem(
            value: l.languageCode,
            child: Text(_languageNames[l.languageCode] ?? l.languageCode),
          ),
      ],
    ));
  }
}

// Menu grid geometry. The small tiles are fixed cards; the private-table card
// spans exactly two of them plus the gap, and its *collapsed* body is pinned to
// one tile height so the row lines up. The header flexes to absorb whatever the
// footer leaves rather than being computed: a button's real height depends on
// Material's tap target and the platform's visual density, so arithmetic here
// would be wrong on some platform. Expanding a sub-action grows the card past
// the pinned body.
const double _menuGap = 16;
const double _menuTileW = 200;
const double _menuTileH = 150;
const double _footerBtnMinH = 44;

/// Which sub-action the private-table card has expanded inline, if any.
enum _TableAction { none, modded, join }

/// The private-table card: one Business-Tour-style card whose split footer
/// carries the three room actions. Create is a single tap (server-default
/// mods - the common case must stay one click); Modded and Join expand
/// *inside* the card, no modal. The mod picker is fed by the server's
/// `list_mods` answer so nobody ever types a mod id; picking order is kept
/// because later mods override earlier ones (ADR-0006) - same tap-to-order
/// chips as the Legal Route builder.
class _PrivateTableCard extends StatefulWidget {
  final GameSession s;
  const _PrivateTableCard({required this.s});

  @override
  State<_PrivateTableCard> createState() => _PrivateTableCardState();
}

class _PrivateTableCardState extends State<_PrivateTableCard> {
  final _code = TextEditingController();
  _TableAction _open = _TableAction.none;
  /// Picked mod ids, in pick order (the order sent on the wire).
  final List<String> _picked = [];

  @override
  void dispose() {
    _code.dispose();
    super.dispose();
  }

  void _toggle(_TableAction a) {
    setState(() => _open = _open == a ? _TableAction.none : a);
    // Lazy: ask for the list only when the picker opens without an answer.
    if (_open == _TableAction.modded && widget.s.availableMods == null) {
      widget.s.requestMods();
    }
  }

  void _join() {
    if (_code.text.trim().isNotEmpty) widget.s.joinGame(_code.text);
  }

  Widget _footerBtn(String label,
      {required VoidCallback onTap,
      bool selected = false,
      bool autofocus = false}) {
    return Expanded(
      child: hoverSfx(TextButton(
        autofocus: autofocus,
        onPressed: onTap,
        style: TextButton.styleFrom(
          foregroundColor: selected ? Pc.gold : Pc.text,
          backgroundColor:
              selected ? Pc.gold.withValues(alpha: 0.12) : null,
          shape: const RoundedRectangleBorder(borderRadius: Pc.radius),
          minimumSize: const Size(0, _footerBtnMinH),
        ),
        child: Text(label,
            style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w700)),
      )),
    );
  }

  Widget _hairline() => Container(width: 1, height: 24, color: Pc.border);

  /// One selectable mod chip; picked chips show their play position, tapping
  /// again removes them (mirrors `_routeChip` on the board).
  Widget _modChip(String id) {
    final pos = _picked.indexOf(id);
    final picked = pos >= 0;
    return hoverSfx(OutlinedButton(
      onPressed: () => setState(() {
        if (picked) {
          _picked.remove(id);
        } else {
          _picked.add(id);
        }
      }),
      style: OutlinedButton.styleFrom(
        shape: const RoundedRectangleBorder(borderRadius: Pc.radius),
        backgroundColor: picked ? Pc.gold.withValues(alpha: 0.3) : null,
        side: BorderSide(color: picked ? Pc.goldDark : Pc.textMuted),
        minimumSize: const Size(0, 40),
      ),
      child: Text(picked ? '$id  #${pos + 1}' : id),
    ));
  }

  Widget _modPicker(AppLocalizations t) {
    final mods = widget.s.availableMods;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (mods == null)
        Text(t.modsLoading,
            style: const TextStyle(fontSize: 12, color: Pc.textMuted))
      else if (mods.isEmpty)
        Text(t.modsUnavailable,
            style: const TextStyle(fontSize: 12, color: Pc.textMuted))
      else ...[
        Text(t.modsOrderHint,
            style: const TextStyle(fontSize: 11, color: Pc.textFaint)),
        const SizedBox(height: 6),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final id in mods) _modChip(id),
        ]),
      ],
      const SizedBox(height: 10),
      // Empty selection sends no `mods` field at all - the server default.
      wideButton(t.create,
          () => widget.s.createGame(mods: List.of(_picked))),
    ]);
  }

  Widget _joinPanel(AppLocalizations t) {
    return Row(children: [
      Expanded(
        child: TextField(
          controller: _code,
          autofocus: true,
          maxLength: 5,
          textCapitalization: TextCapitalization.characters,
          onSubmitted: (_) => _join(),
          decoration: InputDecoration(
              labelText: t.roomCode, counterText: '', isDense: true),
        ),
      ),
      const SizedBox(width: 8),
      hoverSfx(FilledButton(onPressed: _join, child: Text(t.join))),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return SizedBox(
      width: _menuTileW * 2 + _menuGap,
      child: Card(
        // Zero margin so the card's box is exactly the body below - a default
        // Card margin would push it past a tile and break the row.
        margin: EdgeInsets.zero,
        clipBehavior: Clip.antiAlias,
        color: Pc.surface,
        child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                height: _menuTileH,
                child: Column(children: [
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: Row(children: [
                        const Icon(Icons.casino_outlined,
                            size: 40, color: Pc.gold),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(t.menuPrivateTitle,
                                    style: const TextStyle(
                                        fontSize: 18,
                                        fontWeight: FontWeight.w700,
                                        color: Pc.text)),
                                const SizedBox(height: 4),
                                Text(t.menuPrivateSubtitle,
                                    style: const TextStyle(
                                        fontSize: 13, color: Pc.textMuted)),
                              ]),
                        ),
                      ]),
                    ),
                  ),
                  const Divider(height: 1, color: Pc.border),
                  Row(children: [
                    _footerBtn(t.create,
                        autofocus: true, onTap: () => widget.s.createGame()),
                    _hairline(),
                    _footerBtn(t.createModded,
                        selected: _open == _TableAction.modded,
                        onTap: () => _toggle(_TableAction.modded)),
                    _hairline(),
                    _footerBtn(t.join,
                        selected: _open == _TableAction.join,
                        onTap: () => _toggle(_TableAction.join)),
                  ]),
                ]),
              ),
              // The sub-action grows the card in place - never a modal.
              AnimatedSize(
                duration: Motion.establish,
                curve: Motion.deliberate,
                alignment: Alignment.topCenter,
                child: switch (_open) {
                  _TableAction.none => const SizedBox(width: double.infinity),
                  _TableAction.modded => Padding(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
                      child: _modPicker(t)),
                  _TableAction.join => Padding(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
                      child: _joinPanel(t)),
                },
              ),
            ]),
      ),
    );
  }
}

/// One large action card in the main menu. Stateful so it can paint a visible
/// focus ring: on a controller / Steam Deck the player navigates these with
/// the D-pad (arrow keys) and activates with A (Enter/Space, handled by the
/// InkWell), so which tile is selected must be unmistakable.
class _MenuTile extends StatefulWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;
  const _MenuTile({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });

  @override
  State<_MenuTile> createState() => _MenuTileState();
}

class _MenuTileState extends State<_MenuTile> {
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    return hoverSfx(SizedBox(
      width: _menuTileW,
      height: _menuTileH,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 120),
        decoration: BoxDecoration(
          borderRadius: Pc.radius,
          border: Border.all(
            color: _focused ? Pc.gold : Pc.border,
            width: _focused ? 2 : 1,
          ),
        ),
        child: Card(
          margin: EdgeInsets.zero,
          clipBehavior: Clip.antiAlias,
          color: Pc.surface,
          child: InkWell(
            onFocusChange: (f) => setState(() => _focused = f),
            focusColor: Pc.gold.withValues(alpha: 0.12),
            onTap: widget.onTap,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(widget.icon, size: 40, color: Pc.gold),
                  const Spacer(),
                  // The tile is a fixed height, so the labels must be bounded:
                  // a longer translation (French runs longer than English) has
                  // to ellipsize, never overflow the card.
                  Flexible(
                    child: Text(widget.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.w700,
                            color: Pc.text)),
                  ),
                  const SizedBox(height: 4),
                  Flexible(
                    child: Text(widget.subtitle,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                            fontSize: 13, color: Pc.textMuted)),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    ));
  }
}

/// Static rules reference reached from the menu. Deliberately concise: the
/// Business-Tour differences a new player needs, not the full engine spec.
class RulesScreen extends StatelessWidget {
  const RulesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    final sections = <(String, String)>[
      (t.rulesGoalTitle, t.rulesGoalBody),
      (t.rulesMoveTitle, t.rulesMoveBody),
      (t.rulesAuctionTitle, t.rulesAuctionBody),
      (t.rulesBuildTitle, t.rulesBuildBody),
      (t.rulesJailTitle, t.rulesJailBody),
      (t.rulesWinTitle, t.rulesWinBody),
    ];
    // Escape (controller B via Steam Input) pops back to the menu.
    return BackOnEscape(
      child: Scaffold(
        appBar: AppBar(title: Text(t.rulesTitle), backgroundColor: Pc.surface2),
        body: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 640),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(t.rulesTagline,
                      style: const TextStyle(
                          fontSize: 16,
                          fontStyle: FontStyle.italic,
                          color: Pc.gold)),
                  const SizedBox(height: 20),
                  for (final (title, body) in sections) ...[
                    Text(title,
                        style: const TextStyle(
                            fontFamily: 'Fraunces',
                            fontSize: 20,
                            fontWeight: FontWeight.w700,
                            color: Pc.text)),
                    const SizedBox(height: 6),
                    Text(body,
                        style: const TextStyle(
                            fontSize: 15, height: 1.4, color: Pc.text)),
                    const SizedBox(height: 20),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// -- game ----------------------------------------------------------------------

class GameScreen extends StatelessWidget {
  final GameSession s;
  const GameScreen({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    // The action panel lives inside the board's centre, and it holds text
    // fields a player types into. It is built HERE, once per server update, and
    // handed to the stage listener below as a `child` - so an animation frame
    // repaints the board without ever touching it. Sharing one notifier between
    // transient visual state and durable input state is what used to wipe a
    // half-typed bid out from under the player.
    final centre = _CenterPanel(s: s);

    return Scaffold(
      // Motion never gates input, and a player who has seen enough may say so:
      // Escape skips the plan in flight (the remaining beats apply instantly -
      // state is never lost, only its journey).
      body: CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.escape): s.stage.requestSkip,
        },
        child: Focus(
          autofocus: true,
          child: Stack(children: [
            Padding(
              padding: const EdgeInsets.all(12),
              child:
                  Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Expanded(
                  child: Stack(alignment: Alignment.center, children: [
                    // The board subscribes to the stage itself; `centre` is
                    // built out here, so on an animation frame it is the same
                    // widget instance and its element - text fields and all -
                    // is reused untouched.
                    BoardWidget(
                      content: s.content!,
                      view: s.view,
                      mySeat: s.seat,
                      onTileTap: (i) => _tileMenu(context, i),
                      canAct: _hasTileActions,
                      stage: s.stage,
                      highlightTile: s.hoverTile,
                      center: centre,
                    ),
                    ListenableBuilder(
                      listenable: s.stage,
                      builder: (context, _) =>
                          Stack(alignment: Alignment.center, children: [
                        // The played movement card. The one action a player
                        // takes every turn, so it is the one that gets weight.
                        _CardFlash(
                            seq: s.stage.cardSeq, value: s.stage.cardValue),
                        // Card reveals, spotlight and market announcements all
                        // share one banner: same shape, same place, every time.
                        // A player should never have to work out *where* the
                        // game is about to tell them something.
                        _BannerFlash(
                            seq: s.stage.bannerSeq,
                            text: s.stage.bannerText,
                            kind: s.stage.bannerKind),
                      ]),
                    ),
                  ]),
                ),
                const SizedBox(width: 12),
                // The panel grows with the room - open trade offers (up to
                // four per proposer), the post-game survey, the settings
                // expander - so it has to scroll. Not a small-screen nicety:
                // six offers already overflow a 1280x800 Steam Deck.
                // The panel grows with the room - open trade offers (up to
                // four per proposer), the post-game survey, the settings
                // expander - so it has to scroll. Not a small-screen nicety:
                // six offers already overflow a 1280x800 Steam Deck.
                SizedBox(
                  width: 340,
                  child: SingleChildScrollView(child: _SidePanel(s: s)),
                ),
              ]),
            ),
            // Chits crossing from the board to the side panel, and the P1
            // arrest. Above everything, because money travelling from a tile to
            // a seat marker crosses both subtrees - which is exactly why a
            // board-local floater could never express the money rule.
            StageOverlay(stage: s.stage),
          ]),
        ),
      ),
    );
  }

  /// Whether tapping tile `i` would offer at least one action - owning a
  /// tile always does (mortgage/redeem is unconditional below), a rival's
  /// tile only under the same seize conditions `_tileMenu` checks. Drives
  /// both the board's hover outline and the tap guard right below it.
  bool _hasTileActions(int i) {
    final v = s.view;
    final c = s.content;
    if (v == null || c == null) return false;
    final def = c.board[i];
    final ts = v.tiles[i];
    if (ts.owner == s.seat) return true;
    // Buying out a rival's tile you've landed on (ADR-0011/0022): a bare
    // tile is seized at the expropriation premium, a mortgaged one bought
    // out at its flat mortgage value - both go through the same
    // `expropriate` command, both gated on the expropriation rule being on.
    final expro = s.settings?.rules.expropriation ?? c.expropriation;
    return ts.owner != null &&
        ts.owner != s.seat &&
        def.isProperty &&
        expro > 0 &&
        s.myTurn &&
        v.turn.type == 'await_end' &&
        v.players[s.seat!].position == i;
  }

  /// Tile actions: build/sell/boost/mortgage on my tiles (ADR-0012),
  /// expropriate a rival's raw property (ADR-0011).
  void _tileMenu(BuildContext context, int i) {
    if (!_hasTileActions(i)) return; // nothing to offer - don't even open the sheet
    final v = s.view;
    final c = s.content;
    if (v == null || c == null) return;
    final def = c.board[i];
    final ts = v.tiles[i];
    final mine = ts.owner == s.seat;
    final rival = ts.owner != null && ts.owner != s.seat;
    final price = def.price ?? 0;
    // Prefer the live room rules (host may have tweaked them, ADR-0015);
    // fall back to the content snapshot from join.
    final boost = s.settings?.rules.rentBoost ?? c.rentBoost;
    final expro = s.settings?.rules.expropriation ?? c.expropriation;
    final t = AppLocalizations.of(context);

    showModalBottomSheet<void>(
      context: context,
      builder: (ctx) {
        void close() => Navigator.pop(ctx);
        final items = <Widget>[
          ListTile(
              title: Text(def.name,
                  style: const TextStyle(fontWeight: FontWeight.bold))),
        ];
        if (mine) {
          if (def.rentModel == 'houses' && !ts.mortgaged) {
            items.add(ListTile(
                title: Text(t.tileBuildHouse(def.houseCost)),
                onTap: () {
                  s.sendCmd({'type': 'build', 'tile': def.id});
                  close();
                }));
          }
          if (ts.houses > 0) {
            items.add(ListTile(
                title: Text(t.tileSellHouse),
                onTap: () {
                  s.sendCmd({'type': 'sell_house', 'tile': def.id});
                  close();
                }));
          }
          if (boost > 0 && !ts.mortgaged && ts.boosts < 3) {
            items.add(ListTile(
                title: Text(t.tileBoostRent(price * boost ~/ 100)),
                onTap: () {
                  s.sendCmd({'type': 'boost_rent', 'tile': def.id});
                  close();
                }));
          }
          items.add(ListTile(
              title: Text(ts.mortgaged ? t.tileRedeemMortgage : t.tileMortgage),
              onTap: () {
                s.sendCmd({
                  'type': ts.mortgaged ? 'unmortgage' : 'mortgage',
                  'tile': def.id
                });
                close();
              }));
        } else if (rival &&
            def.isProperty &&
            expro > 0 &&
            s.myTurn &&
            v.turn.type == 'await_end' &&
            v.players[s.seat!].position == i) {
          // A mortgaged rival tile is bought out at its flat mortgage
          // value (price/2), transferring still mortgaged (ADR-0022
          // amended). A bare tile is seized at the expropriation premium;
          // improved tiles liquidate on seizure, the former owner refunded
          // half cost per level on top of compensation (ADR-0022).
          final String label;
          final String subtitle;
          if (ts.mortgaged) {
            label = t.tileBuyOutMortgage(price ~/ 2);
            subtitle = t.tileBuyOutMortgageSub;
          } else if (ts.houses > 0) {
            label = t.tileSeizeLiquidate(price * expro ~/ 100);
            subtitle = t.tileSeizeSub;
          } else {
            label = t.tileSeize(price * expro ~/ 100);
            subtitle = t.tileSeizeSub;
          }
          items.add(ListTile(
              title: Text(label),
              subtitle: Text(subtitle),
              onTap: () {
                s.sendCmd({'type': 'expropriate', 'tile': def.id});
                close();
              }));
        }
        if (items.length == 1) return const SizedBox.shrink();
        return SafeArea(child: Wrap(children: items));
      },
    );
  }
}

/// Status line, contextual action buttons, and the event log — lives in the
/// middle of the board, like the reference client.
class _CenterPanel extends StatelessWidget {
  final GameSession s;
  const _CenterPanel({required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    // A dark plate on the sage plaza: the HUD is a panel *on* the board, not a
    // hole in it. (The plaza itself stays sage - `docs/visual-identity.md`.)
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: Pc.surface,
        borderRadius: Pc.radius,
        border: Border.all(color: Pc.goldDark, width: 1.5),
      ),
      child: DefaultTextStyle(
        style: const TextStyle(color: Pc.text, fontSize: 13),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            // The wordmark yields first when the board's centre gets tight:
            // the clocks and toggles beside it are functional, it is not.
            const Flexible(
              child: Text('PARCELLO',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.bold,
                      letterSpacing: 3,
                      color: Pc.gold)),
            ),
            const Spacer(),
            // Shown for the whole game, end included: the final time left is
            // part of the result (a bankruptcy win keeps time on the clock).
            if (s.gameEndsAt != null) ...[
              _Countdown(endsAt: s.gameEndsAt!),
              const SizedBox(width: 8),
            ],
            _MotionButton(s: s),
            const _MuteButton(),
          ]),
        const SizedBox(height: 4),
        Row(children: [
          Expanded(
              child: Text(_status(t),
                  style: const TextStyle(fontWeight: FontWeight.w600))),
          if (s.turnEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            _Countdown(
                endsAt: s.turnEndsAt!,
                icon: Icons.hourglass_bottom,
                warnSecs: 10,
                // The server's own clock only starts once this seat's
                // render ack lands (ADR-0028) - the display must not look
                // like movement/animation is eating thinking time.
                paused: s.isAnimating),
          ],
          // Personal time bank (ADR-0023): a flat reserve for the whole
          // plain turn window, then counts down to the hard stop. Never
          // refilled.
          if (s.bankEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            _Countdown(
                endsAt: s.bankEndsAt!,
                holdUntil: s.turnEndsAt,
                icon: Icons.account_balance,
                warnSecs: 10,
                paused: s.isAnimating),
          ],
          // Sealed-bid window (ADR-0018): a one-shot ~12s countdown, local
          // estimate only - the server alone decides when it actually
          // closes, and its clock waits for the whole table's acks
          // (ADR-0028).
          if (s.bidEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            _Countdown(
                endsAt: s.bidEndsAt!,
                icon: Icons.gavel,
                warnSecs: 3,
                paused: s.isAnimating),
          ],
          // Corruption bribe vote window (ADR-0024): same pattern.
          if (s.voteEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            _Countdown(
                endsAt: s.voteEndsAt!,
                icon: Icons.how_to_vote,
                warnSecs: 2,
                paused: s.isAnimating),
          ],
        ]),
          if (_poolsLine(t) != null) ...[
            const SizedBox(height: 2),
            Text(_poolsLine(t)!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_forecastLine(t) != null) ...[
            const SizedBox(height: 2),
            Text(_forecastLine(t)!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_spotlightLine(t) != null) ...[
            const SizedBox(height: 2),
            Text(_spotlightLine(t)!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_vpLegend(t) != null) ...[
            const SizedBox(height: 6),
            _vpLegend(t)!,
          ],
          const SizedBox(height: 6),
          _Actions(s: s),
          const SizedBox(height: 6),
          Expanded(child: _EventLog(log: s.log)),
        ]),
      ),
    );
  }

  /// How victory points are earned (ADR-0020), front and center on the
  /// table - the race is the win condition but its scoring was opaque in
  /// playtests (2026-07). Null when the VP race is off.
  Widget? _vpLegend(AppLocalizations t) {
    final target = s.content?.winVictoryPoints ?? 0;
    if (s.view == null || target <= 0) return null;
    final rows = [
      ('1', t.vpLegendUtilityTile),
      ('2', t.vpLegendMaxedTile),
      ('3', t.vpLegendFullGroup),
      ('+2', t.vpLegendRoundBonus),
    ];
    return Container(
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: Pc.gold.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: Pc.gold, width: 1),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(t.vpLegendHeader(target),
            style: const TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.bold,
                color: Pc.goldDark,
                letterSpacing: 1)),
        const SizedBox(height: 3),
        for (final (pts, what) in rows)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 1),
            child: Row(children: [
              SizedBox(
                width: 24,
                child: Text(pts,
                    style: const TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.bold,
                        color: Pc.goldDark)),
              ),
              Expanded(
                child: Text(what,
                    style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
              ),
            ]),
          ),
        ..._roundProgress(t),
      ]),
    );
  }

  /// Live state of the round metronome (ADR-0020), so the `+2` above stops
  /// looking like it arrives out of nowhere: a "round" completes when every
  /// surviving player has cycled a full hand of movement cards, and the
  /// bonus banks to whoever is richest at that instant. The round number is
  /// the MINIMUM hands-cycled across survivors - so progress is simply how
  /// many players have already pulled ahead of that minimum.
  List<Widget> _roundProgress(AppLocalizations t) {
    final v = s.view;
    if (v == null || v.finished) return const [];
    final alive = [
      for (var i = 0; i < v.players.length; i++)
        if (!v.players[i].bankrupt) i,
    ];
    if (alive.isEmpty) return const [];
    final round =
        alive.map((i) => v.players[i].handsCycled).reduce((a, b) => a < b ? a : b);
    final done = alive.where((i) => v.players[i].handsCycled > round).toList();
    // Whoever would bank the +2 if the round closed right now: strictly
    // richest, ties to the lowest seat (mirrors `award_round_bonus`).
    var leader = alive.first;
    for (final i in alive) {
      if (v.players[i].cash > v.players[leader].cash) leader = i;
    }
    return [
      const SizedBox(height: 6),
      const Divider(height: 1, color: Color(0x33A9812F)),
      const SizedBox(height: 5),
      Row(children: [
        Text(t.roundLabel(round + 1),
            style: const TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.bold,
                color: Pc.goldDark,
                letterSpacing: 1)),
        const SizedBox(width: 8),
        // One pip per surviving player: filled once they have cycled their
        // hand for this round. All filled = the bonus fires.
        for (final i in alive)
          Container(
            width: 10,
            height: 10,
            margin: const EdgeInsets.only(right: 3),
            decoration: BoxDecoration(
              color: done.contains(i)
                  ? pawnColor(i)
                  : Colors.transparent,
              shape: BoxShape.circle,
              border: Border.all(
                  color: pawnColor(i), width: 1.5),
            ),
          ),
        const SizedBox(width: 4),
        // The pips already say who the table waits on; the count is the part
        // that can be clipped when the centre is tight.
        Flexible(
          child: Text(t.roundHandsCycled(done.length, alive.length),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 10, color: Pc.textFaint)),
        ),
      ]),
      const SizedBox(height: 2),
      Text(
        t.roundBonusHint(s.playerName(leader)),
        style: const TextStyle(fontSize: 10, color: Pc.textMuted),
      ),
    ];
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

  String _status(AppLocalizations t) {
    final v = s.view;
    if (v == null) {
      return s.seats.length >= 2
          ? t.statusReadyHostCanStart
          : t.statusWaitingForPlayers;
    }
    if (v.finished) return t.statusGameOver(s.playerName(v.winner!));
    final turn = v.turn;
    switch (turn.type) {
      case 'blind_auction':
        final pending = <int>[
          for (var i = 0; i < turn.bids.length; i++)
            if (turn.bids[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? t.statusNobody
            : pending.map(s.playerName).join(', ');
        return t.statusSealedBid(s.tileName(turn.tile!), waiting);
      case 'bribe_vote':
        final pending = <int>[
          for (var i = 0; i < turn.votes.length; i++)
            if (i != turn.briber && turn.votes[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? t.statusNobody
            : pending.map(s.playerName).join(', ');
        return t.statusBribeVote(
            s.playerName(turn.briber!), turn.amount!, waiting);
      default:
        return t.statusPlayerTurn(s.playerName(v.current));
    }
  }
}

/// Ticking countdown to a deadline. Used for the timed-game clock
/// (ADR-0010), the per-turn AFK timer, and the personal time bank
/// (ADR-0023); turns red under `warnSecs`.
class _Countdown extends StatefulWidget {
  final DateTime endsAt;
  final IconData icon;
  final int warnSecs;
  /// While now is before `holdUntil`, the displayed value freezes at
  /// `endsAt - holdUntil` instead of ticking down from `endsAt - now` - the
  /// personal time bank must read as a flat reserve for the whole plain
  /// turn window and only start draining once that window is spent
  /// (ADR-0023), not from the moment the turn begins.
  final DateTime? holdUntil;
  /// While true, freezes the display at whatever it last showed instead of
  /// ticking down (ADR-0028): none of these server timers are actually
  /// running while the table is still rendering an Update, so the display
  /// must not look like it is - a fresh deadline always follows once the
  /// animation settles, at which point this naturally shows the full
  /// duration again rather than jumping.
  final bool paused;
  const _Countdown(
      {required this.endsAt,
      this.icon = Icons.timer,
      this.warnSecs = 60,
      this.holdUntil,
      this.paused = false});

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  // Seconds-remaining values worth a countdown cue: the final stretch plus
  // the "heads up" marks further out.
  static const _milestones = {60, 30, 10, 5, 4, 3, 2, 1, 0};

  Timer? _timer;
  int? _lastTicked;
  int? _frozenSecs;

  int _secsLeft() {
    if (widget.paused) return _frozenSecs ?? _liveSecsLeft();
    return _frozenSecs = _liveSecsLeft();
  }

  int _liveSecsLeft() {
    final now = DateTime.now();
    final holdUntil = widget.holdUntil;
    final reference =
        (holdUntil != null && now.isBefore(holdUntil)) ? holdUntil : now;
    final left = widget.endsAt.difference(reference);
    return left.isNegative ? 0 : left.inSeconds;
  }

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (widget.paused) return; // no tick cue while frozen
      final secs = _secsLeft();
      if (secs != _lastTicked && _milestones.contains(secs)) {
        _lastTicked = secs;
        sfx.timerTick();
      }
      setState(() {});
    });
  }

  @override
  void didUpdateWidget(covariant _Countdown old) {
    super.didUpdateWidget(old);
    // A new deadline (next turn, restarted game clock) resets the cues.
    if (old.endsAt != widget.endsAt) {
      _lastTicked = null;
      _frozenSecs = null;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final secs = _secsLeft();
    final mmss =
        '${(secs ~/ 60).toString().padLeft(2, '0')}:${(secs % 60).toString().padLeft(2, '0')}';
    final warn = secs <= widget.warnSecs;
    final color =
        warn ? Pc.oxblood : Pc.text;
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Icon(widget.icon, size: 18, color: color),
      const SizedBox(width: 4),
      Text(mmss,
          style: TextStyle(
            fontWeight: FontWeight.bold,
            fontFeatures: const [FontFeature.tabularFigures()],
            color: color,
          )),
    ]);
  }
}

/// Toggles sound effects on/off (`sfx.enabled`).
class _MuteButton extends StatefulWidget {
  const _MuteButton();

  @override
  State<_MuteButton> createState() => _MuteButtonState();
}

class _MuteButtonState extends State<_MuteButton> {
  @override
  Widget build(BuildContext context) {
    return hoverSfx(IconButton(
      iconSize: 18,
      padding: EdgeInsets.zero,
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(),
      tooltip: sfx.enabled
          ? AppLocalizations.of(context).muteSound
          : AppLocalizations.of(context).unmuteSound,
      icon: Icon(sfx.enabled ? Icons.volume_up : Icons.volume_off,
          color: Pc.textMuted),
      onPressed: () => setState(() => sfx.enabled = !sfx.enabled),
    ));
  }
}

/// The accessibility knob (ADR-0030): full -> reduced -> instant.
///
/// `instant` is not a degraded mode. It is the same "I do not animate" path the
/// CLI and bot seats already take under ADR-0028, which is why the server needs
/// no change to tolerate it - and why nothing in the game is ever conveyed by
/// motion alone. Pause on any frame and the game is still playable.
class _MotionButton extends StatefulWidget {
  final GameSession s;
  const _MotionButton({required this.s});

  @override
  State<_MotionButton> createState() => _MotionButtonState();
}

class _MotionButtonState extends State<_MotionButton> {
  static const _icons = {
    MotionProfile.full: Icons.animation,
    MotionProfile.reduced: Icons.slow_motion_video,
    MotionProfile.instant: Icons.bolt,
  };

  @override
  Widget build(BuildContext context) {
    final stage = widget.s.stage;
    return hoverSfx(IconButton(
      iconSize: 18,
      padding: EdgeInsets.zero,
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(),
      tooltip: 'Motion: ${stage.profile.name}',
      icon: Icon(_icons[stage.profile], color: Pc.textMuted),
      onPressed: () => setState(() {
        const cycle = MotionProfile.values;
        stage.profile = cycle[(stage.profile.index + 1) % cycle.length];
      }),
    ));
  }
}

/// The played movement card value, shown big in the middle of the board for
/// a moment after each play, then faded out (ADR-0017; like a physical board
/// game's dice result, replaced by a card since movement no longer rolls).
class _CardFlash extends StatefulWidget {
  final int seq, value;
  const _CardFlash({required this.seq, required this.value});

  @override
  State<_CardFlash> createState() => _CardFlashState();
}

class _CardFlashState extends State<_CardFlash> {
  bool _visible = false;
  Timer? _timer;

  @override
  void didUpdateWidget(_CardFlash old) {
    super.didUpdateWidget(old);
    if (widget.seq != old.seq && widget.seq > 0) {
      setState(() => _visible = true);
      _timer?.cancel();
      _timer = Timer(const Duration(milliseconds: 1500), () {
        if (mounted) setState(() => _visible = false);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: AnimatedOpacity(
        opacity: _visible ? 1 : 0,
        duration: Motion.cardPlay,
        curve: Motion.arrive,
        child: Container(
          width: 66,
          height: 66,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: Pc.parchment,
            borderRadius: Pc.radius,
            border: Border.all(color: Pc.goldDark, width: 1.5),
            boxShadow: Pc.hairShadow,
          ),
          child: Text(
            '${widget.value}',
            style: const TextStyle(
                fontSize: 32,
                fontWeight: FontWeight.bold,
                color: Pc.parchmentInk,
                fontFeatures: [FontFeature.tabularFigures()]),
          ),
        ),
      ),
    );
  }
}

/// A one-shot banner over the board: a drawn card, a spotlight, a market event.
/// One shape, one place, every time - a player should never have to work out
/// *where* the game is going to tell them something.
class _BannerFlash extends StatefulWidget {
  final int seq;
  final String text;
  final BannerKind kind;
  const _BannerFlash({
    required this.seq,
    required this.text,
    required this.kind,
  });

  @override
  State<_BannerFlash> createState() => _BannerFlashState();
}

class _BannerFlashState extends State<_BannerFlash> {
  bool _visible = false;
  Timer? _timer;

  @override
  void didUpdateWidget(_BannerFlash old) {
    super.didUpdateWidget(old);
    if (widget.seq != old.seq && widget.seq > 0) {
      setState(() => _visible = true);
      _timer?.cancel();
      // Held for as long as the beat the director paid for - the two must agree,
      // or the banner outlives the pause that exists to let it be read.
      final hold = widget.kind == BannerKind.card
          ? Motion.cardReveal
          : Motion.banner;
      _timer = Timer(hold, () {
        if (mounted) setState(() => _visible = false);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Paper for a card read; a dark plate for a world event. The register tells
    // you which kind of thing just happened before you read a word of it.
    final paper = widget.kind == BannerKind.card;
    return IgnorePointer(
      child: AnimatedOpacity(
        opacity: _visible ? 1 : 0,
        duration: Motion.ambient,
        child: Container(
          constraints: const BoxConstraints(maxWidth: 320),
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
          decoration: BoxDecoration(
            color: paper ? Pc.parchment : Pc.surface,
            borderRadius: Pc.radius,
            border: Border.all(color: Pc.goldDark, width: 1.5),
            boxShadow: Pc.hairShadow,
          ),
          child: Text(
            widget.text,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w600,
              color: paper ? Pc.parchmentInk : Pc.text,
            ),
          ),
        ),
      ),
    );
  }
}

/// Caps a numeric text field at `max`, clamping down any edit that would
/// exceed it (used for the sealed-bid amount, bounded by the seat's cash).
/// Empty input passes through so the field can be cleared and retyped.
class _MaxValueFormatter extends TextInputFormatter {
  final int max;
  const _MaxValueFormatter(this.max);

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

class _Actions extends StatefulWidget {
  final GameSession s;
  const _Actions({required this.s});

  @override
  State<_Actions> createState() => _ActionsState();
}

class _ActionsState extends State<_Actions> {
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

    final touch = ButtonStyle(
      minimumSize: WidgetStateProperty.all(const Size(0, 46)),
      padding: WidgetStateProperty.all(
          const EdgeInsets.symmetric(horizontal: 18)),
      textStyle: WidgetStateProperty.all(const TextStyle(fontSize: 15)),
    );
    Widget btn(String label, Map<String, dynamic> cmd, {bool primary = true}) {
      return hoverSfx(primary
          ? FilledButton(
              onPressed: () => s.sendCmd(cmd), style: touch, child: Text(label))
          : OutlinedButton(
              onPressed: () => s.sendCmd(cmd), style: touch, child: Text(label)));
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
      if (_bidInitTile != t.tile) {
        // Seed at that price, but never above what you can actually
        // bid (the sealed-bid invariant validates against cash, ADR-0018).
        _bid.text = '${price.clamp(0, cash)}';
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
          style: const TextStyle(fontSize: 12),
        ),
        // The discoverer's edge (ADR-0018): landing there took the risk,
        // so a contested win above the floor is rewarded with a discount.
        if (isDiscoverer)
          Text(
            loc.actionDiscovererHint,
            style: const TextStyle(fontSize: 10, color: Pc.textFaint),
          ),
        SizedBox(
          width: 90,
          child: TextField(
            controller: _bid,
            keyboardType: TextInputType.number,
            // Digits only, and never more than the seat can afford - the
            // field itself refuses an over-cash bid as you type (2026-07).
            inputFormatters: [
              FilteringTextInputFormatter.digitsOnly,
              _MaxValueFormatter(cash),
            ],
            style: const TextStyle(color: Pc.text),
            decoration: const InputDecoration(isDense: true),
          ),
        ),
        hoverSfx(FilledButton(
          // Clamp at submit too, belt-and-suspenders: the field is already
          // capped, but the amount on the wire must never exceed cash.
          onPressed: () => s.sendCmd({
            'type': 'submit_blind_bid',
            'amount': (int.tryParse(_bid.text) ?? 0).clamp(0, cash),
          }),
          child: Text(loc.actionBid),
        )),
        btn(loc.actionAbstain, {'type': 'submit_blind_bid', 'amount': 0},
            primary: false),
        // Quick raises as a percent of the list price, so escalating a bid
        // doesn't mean typing out full numbers under the clock. Mutating
        // the controller already repaints the TextField bound to it - no
        // setState needed (and one less rebuild to guard against).
        for (final pct in [10, 25, 50, 100])
          hoverSfx(OutlinedButton(
            onPressed: () => bumpBid(pct),
            style: touch,
            child: Text(loc.actionRaisePct(pct)),
          )),
        // All-in: the highest bid the sealed-bid invariant will accept.
        hoverSfx(OutlinedButton(
          onPressed: () => _bid.text = '$cash',
          style: touch,
          child: Text(loc.actionMaxBid(cash)),
        )),
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
          style: const TextStyle(fontSize: 12),
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
                  style: const TextStyle(fontSize: 12)),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final value in sorted) _routeChip(value, touch),
                ],
              ),
              Row(mainAxisSize: MainAxisSize.min, children: [
                hoverSfx(OutlinedButton(
                  onPressed: routeComplete
                      ? () {
                          s.sendCmd({
                            'type': 'choose_legal_route',
                            'order': _routeOrder,
                          });
                          setState(() => _routeOrder.clear());
                        }
                      : null,
                  style: touch,
                  child: Text(loc.actionChooseRoute),
                )),
                if (_routeOrder.isNotEmpty) ...[
                  const SizedBox(width: 6),
                  hoverSfx(TextButton(
                    onPressed: () => setState(() => _routeOrder.clear()),
                    child: Text(loc.actionReset),
                  )),
                ],
              ]),
              SizedBox(
                width: 90,
                child: TextField(
                  controller: _bribe,
                  keyboardType: TextInputType.number,
                  style: const TextStyle(color: Pc.text),
                  decoration: const InputDecoration(isDense: true),
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
          style: const TextStyle(color: Pc.textFaint, fontSize: 11)));
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
  Widget _routeChip(int value, ButtonStyle style) {
    final pos = _routeOrder.indexOf(value);
    final picked = pos >= 0;
    return hoverSfx(OutlinedButton(
      onPressed: () => setState(() {
        if (picked) {
          _routeOrder.remove(value);
        } else {
          _routeOrder.add(value);
        }
      }),
      style: style.copyWith(
        backgroundColor: WidgetStateProperty.all(
            picked ? Pc.gold.withValues(alpha: 0.3) : null),
        side: WidgetStateProperty.all(BorderSide(
            color:
                picked ? Pc.goldDark : Pc.textMuted)),
      ),
      child: Text(picked ? '$value  #${pos + 1}' : '$value'),
    ));
  }
}

class _EventLog extends StatelessWidget {
  final List<String> log;
  const _EventLog({required this.log});

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Pc.bg,
        border: Border.all(color: Pc.border),
        borderRadius: BorderRadius.circular(4),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: ListView.builder(
        reverse: true, // newest visible without scroll management
        itemCount: log.length,
        itemBuilder: (ctx, i) => Text(
          log[log.length - 1 - i],
          style: const TextStyle(fontSize: 11, color: Pc.textMuted),
        ),
      ),
    );
  }
}

// -- side panel ------------------------------------------------------------------

class _SidePanel extends StatelessWidget {
  final GameSession s;
  const _SidePanel({required this.s});

  @override
  Widget build(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      // Game over: replay together, or go back to the start screen.
      if (v != null && v.finished)
        Card(
          color: Pc.surface2,
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(t.sideWinnerWins(s.playerName(v.winner!)),
                      style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: 8),
                  Row(children: [
                    Expanded(child: wideButton(t.playAgain, s.sendPlayAgain)),
                    const SizedBox(width: 8),
                    Expanded(
                        child: wideButton(t.continueLabel, s.leaveRoom,
                            primary: false)),
                  ]),
                  Text(t.playAgainHint,
                      style: const TextStyle(
                          fontSize: 11, color: Pc.textMuted)),
                ]),
          ),
        ),
      Card(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Expanded(
                child: Text(t.sideRoom(s.code ?? ''),
                    style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.bold,
                        color: Pc.gold,
                        letterSpacing: 2)),
              ),
              if (s.code != null)
                hoverSfx(IconButton(
                  iconSize: 18,
                  visualDensity: VisualDensity.compact,
                  tooltip: t.copyRoomCode,
                  icon: const Icon(Icons.copy, color: Pc.textMuted),
                  onPressed: () => copyCode(context, s.code!),
                )),
            ]),
            const SizedBox(height: 6),
            // The seat list is the only part of the side panel the stage drives
            // (chit anchors, the sealed-bid reveal), so it is the only part that
            // repaints on an animation frame. The trade panel and the settings
            // fields below never do.
            ListenableBuilder(
                listenable: s.stage, builder: (context, _) => _players(t)),
            if (s.view == null) ...[
              const SizedBox(height: 8),
              wideButton(t.startGame,
                  s.seat == 0 && s.seats.length >= 2 ? s.sendStart : null),
              // Host-only bot controls. Bots fill empty seats but yield to
              // humans, so they never block a join (ADR-0014).
              if (s.seat == 0)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Row(children: [
                    Expanded(
                        child: wideButton(t.addBot,
                            s.seats.length < 6 ? s.addBot : null,
                            primary: false)),
                    const SizedBox(width: 6),
                    Expanded(
                        child: wideButton(t.removeBot,
                            s.seats.any((x) => x.isBot) ? s.removeBot : null,
                            primary: false)),
                  ]),
                ),
              if (s.code != null)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: wideButton(t.copyCodeToShare, () => copyCode(context, s.code!),
                      primary: false),
                ),
              if (s.settings != null) _SettingsPanel(s: s),
              // Cancel: leave the room (dissolves it for the host) and return
              // to the main menu. Keyboard/controller reachable like any button.
              const SizedBox(height: 6),
              wideButton(t.backToMenu, s.leaveRoom, primary: false),
            ],
          ]),
        ),
      ),
      Card(
          child: Padding(
              padding: const EdgeInsets.all(12), child: _trades(context))),
      // Post-game survey: an ordinary side card, never a modal - it must
      // not block anything (no frustration by design).
      if (s.view?.finished == true && !s.feedbackDone) _FeedbackCard(s: s),
      Card(
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: hoverSfx(OutlinedButton(
            style: OutlinedButton.styleFrom(
                foregroundColor: Pc.oxblood),
            onPressed: () async {
              final ok = await showDialog<bool>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: Text(t.resignConfirmTitle),
                  actions: [
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonNo();
                          Navigator.pop(ctx, false);
                        },
                        child: Text(t.cancel))),
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonYes();
                          Navigator.pop(ctx, true);
                        },
                        child: Text(t.resign))),
                  ],
                ),
              );
              if (ok == true) s.sendCmd({'type': 'resign'});
            },
            child: Text(t.resign),
          )),
        ),
      ),
    ]);
  }

  /// VP leaderboard rank per seat (1 = leading), null for bankrupt seats
  /// or when the VP race is off. Ties break to the lowest seat, matching
  /// every tiebreak in the engine.
  List<int?> _vpRanks(ClientView v) {
    final ranks = List<int?>.filled(v.players.length, null);
    if ((s.content?.winVictoryPoints ?? 0) <= 0) return ranks;
    final alive = [
      for (var i = 0; i < v.players.length; i++)
        if (!v.players[i].bankrupt) i,
    ]..sort((a, b) {
        final byVp = v.players[b].victoryPoints - v.players[a].victoryPoints;
        return byVp != 0 ? byVp : a - b;
      });
    for (var r = 0; r < alive.length; r++) {
      ranks[alive[r]] = r + 1;
    }
    return ranks;
  }

  Widget _players(AppLocalizations t) {
    final v = s.view;
    final rows = <Widget>[];
    final count = v?.players.length ?? s.seats.length;
    final ranks = v != null ? _vpRanks(v) : List<int?>.filled(count, null);
    // Round metronome (ADR-0020): the round is the minimum hands-cycled
    // across survivors, so anyone above that minimum has already done their
    // hand this round - tag them so it is obvious who the table waits on.
    final int? round = (v == null || v.finished)
        ? null
        : v.players
            .asMap()
            .entries
            .where((e) => !e.value.bankrupt)
            .map((e) => e.value.handsCycled)
            .fold<int?>(null, (m, h) => m == null || h < m ? h : m);
    for (var i = 0; i < count; i++) {
      final p = v?.players.elementAtOrNull(i);
      final seatInfo = s.seats.elementAtOrNull(i);
      final name = p?.name ?? seatInfo?.name ?? t.seatFallback(i);
      // Whose turn is it: bold text alone read as too subtle in playtests
      // (2026-07) - a highlighted row + a leading marker reads at a glance.
      final isActive = v != null && !v.finished && v.current == i;
      final rank = ranks[i];
      final cycled =
          round != null && p != null && !p.bankrupt && p.handsCycled > round;
      final tags = [
        if (cycled) t.playerTagHandCycled,
        if (i == s.seat) t.playerTagYou,
        if (p?.inJail == true) t.playerTagJail,
        if (p?.jailRoute != null) t.playerTagRoute(p!.jailRoute!.join(',')),
        if ((p?.jailCards ?? 0) > 0) t.playerTagJailCard(p!.jailCards),
        if (seatInfo?.isBot == true)
          t.playerTagBot
        else if (seatInfo?.connected == false)
          t.playerTagOffline,
      ].join(' ');
      rows.add(AnimatedContainer(
        duration: const Duration(milliseconds: 200),
        margin: const EdgeInsets.symmetric(vertical: 2),
        padding: EdgeInsets.symmetric(horizontal: 6, vertical: isActive ? 5 : 2),
        decoration: BoxDecoration(
          color: isActive
              ? Pc.gold.withValues(alpha: 0.16)
              : null,
          borderRadius: BorderRadius.circular(4),
          border: Border(
            left: BorderSide(
              color: isActive ? Pc.gold : Colors.transparent,
              width: 3,
            ),
          ),
        ),
        child: Opacity(
          opacity: p?.bankrupt == true ? 0.4 : 1,
          child: Row(children: [
            SizedBox(
              width: 16,
              child: isActive
                  ? const Icon(Icons.play_arrow,
                      size: 16, color: Pc.goldDark)
                  : null,
            ),
            // Pawn circle doubles as the live VP leaderboard - and as the
            // anchor every chit addressed to this player flies to. Money that
            // lands somewhere is money you can see arriving.
            Container(
              key: s.stage.anchors.seatKey(i),
              width: 18,
              height: 18,
              alignment: Alignment.center,
              decoration:
                  BoxDecoration(color: pawnColor(i), shape: BoxShape.circle),
              child: rank == null
                  ? null
                  : rank == 1
                      ? const Icon(Icons.workspace_premium,
                          size: 12, color: Pc.text)
                      : Text('$rank',
                          style: const TextStyle(
                              fontSize: 10,
                              fontWeight: FontWeight.bold,
                              color: Pc.text)),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text('$name $tags',
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontWeight: isActive ? FontWeight.bold : null,
                    decoration:
                        p?.bankrupt == true ? TextDecoration.lineThrough : null,
                  )),
            ),
            // A sealed bid, face-up (ADR-0018). Every seat's bid flips at once
            // and is held long enough to compare - this is the single most
            // information-dense moment in Parcello, and the old client never
            // rendered it at all: the auction just silently resolved.
            if (s.stage.bidReveal case final r?)
              if (i < r.bids.length) _BidChip(bid: r.bids[i], won: r.winner == i),
            if (p != null)
              Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
                Text('\$${p.cash}',
                    style: const TextStyle(
                        fontFeatures: [FontFeature.tabularFigures()])),
                // Net worth decides a timed game (ADR-0010), so surface it then.
                if (s.gameEndsAt != null)
                  Text(t.sideNetWorth(s.netWorth(i)),
                      style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
                // Victory-point race (ADR-0020): "the race IS the game".
                if ((s.content?.winVictoryPoints ?? 0) > 0)
                  Text(t.sideVictoryPoints(p.victoryPoints, s.content!.winVictoryPoints),
                      style: const TextStyle(
                          fontSize: 11,
                          color: Pc.goldDark,
                          fontWeight: FontWeight.w700,
                          fontFeatures: [FontFeature.tabularFigures()])),
              ]),
          ]),
        ),
      ));
    }
    // The VP scoring breakdown lives in the center panel now
    // (`_CenterPanel._vpLegend`), where it reads at the table's focus.
    return Column(children: rows);
  }

  Widget _trades(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    final offers = v?.pendingTrades ?? [];
    String side(int cash, List<int> tiles) {
      final parts = [
        if (cash > 0) '\$$cash',
        ...tiles.map(s.tileName),
      ];
      return parts.isEmpty ? t.tradeNothing : parts.join(' + ');
    }

    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(t.tradesHeader,
          style: const TextStyle(
              fontSize: 12, color: Pc.textMuted, letterSpacing: 1)),
      const SizedBox(height: 6),
      if (offers.isEmpty)
        Text(t.tradeNoOffers, style: const TextStyle(color: Pc.textMuted)),
      for (final o in offers)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(t.tradeOffer(
                o.id,
                s.playerName(o.from),
                side(o.giveCash, o.giveTiles),
                side(o.receiveCash, o.receiveTiles),
                s.playerName(o.to))),
            Row(children: [
              if (o.to == s.seat) ...[
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'accept_trade', 'trade': o.id}),
                    child: Text(t.actionAccept))),
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'decline_trade', 'trade': o.id}),
                    child: Text(t.tradeRefuse))),
              ],
              if (o.from == s.seat)
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'cancel_trade', 'trade': o.id}),
                    child: Text(t.cancel))),
            ]),
          ]),
        ),
      if (v != null && !v.finished)
        hoverSfx(OutlinedButton(
          onPressed: () => showDialog<void>(
              context: context, builder: (ctx) => TradeDialog(s: s)),
          child: Text(t.tradeNewOffer),
        )),
    ]);
  }
}

/// One seat's sealed bid, revealed (ADR-0018). Flips up on the seat marker, in
/// the same instant as everyone else's, and holds - the hold is what makes a
/// simultaneous decision comparable, which is the whole point of showing it.
class _BidChip extends StatelessWidget {
  final int bid;
  final bool won;
  const _BidChip({required this.bid, required this.won});

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: Motion.bidReveal,
      curve: Motion.arrive,
      builder: (context, t, child) => Transform(
        alignment: Alignment.center,
        // A card turning over, not a number appearing.
        transform: Matrix4.identity()..rotateX((1 - t) * 1.4),
        child: Opacity(opacity: t, child: child),
      ),
      child: Container(
        margin: const EdgeInsets.only(right: 6),
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
        decoration: BoxDecoration(
          color: won ? Pc.gold : Pc.parchment,
          borderRadius: Pc.radius,
          border: Border.all(color: won ? Pc.goldDark : Pc.border),
        ),
        child: Text(
          // Zero is an abstention, and it reads as one.
          bid == 0 ? '--' : '\$$bid',
          style: const TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w800,
            color: Pc.parchmentInk,
            fontFeatures: [FontFeature.tabularFigures()],
          ),
        ),
      ),
    );
  }
}

/// Lobby settings panel (ADR-0015): the host (seat 0) edits timers and rules
/// for this game; everyone else sees them read-only. Collapsed by default so
/// the lobby stays tidy. Settings freeze once the game starts.
class _SettingsPanel extends StatefulWidget {
  final GameSession s;
  const _SettingsPanel({required this.s});

  @override
  State<_SettingsPanel> createState() => _SettingsPanelState();
}

class _SettingsPanelState extends State<_SettingsPanel> {
  // Field keys in display order; labels are resolved per-locale in _hostLabel.
  static const _fieldKeys = [
    'game',
    'turn',
    'bank',
    'starting_balance',
    'go_salary',
    'velocity_min',
    'velocity_max',
    'max_houses',
    'bankruptcy_threshold',
    'expropriation',
    'rent_boost',
    'win_full_groups',
    'win_points',
    'subsidiary_pool',
    'conglomerate_pool',
    'spotlight_rent_pct',
    'spotlight_duration',
  ];
  late final Map<String, TextEditingController> _c;

  String _hostLabel(AppLocalizations t, String key) => switch (key) {
        'game' => t.settingGameLength,
        'turn' => t.settingTurnLimit,
        'bank' => t.settingTimeBank,
        'starting_balance' => t.settingStartingBalance,
        'go_salary' => t.settingGoSalary,
        'velocity_min' => t.settingVelocityMin,
        'velocity_max' => t.settingVelocityMax,
        'max_houses' => t.settingMaxHouses,
        'bankruptcy_threshold' => t.settingBankruptcyThreshold,
        'expropriation' => t.settingExpropriationPct,
        'rent_boost' => t.settingRentBoostPct,
        'win_full_groups' => t.settingDominationGroups,
        'win_points' => t.settingVictoryPointsTarget,
        'subsidiary_pool' => t.settingSubsidiaryPool,
        'conglomerate_pool' => t.settingConglomeratePool,
        'spotlight_rent_pct' => t.settingSpotlightRentPct,
        'spotlight_duration' => t.settingSpotlightDuration,
        _ => key,
      };

  @override
  void initState() {
    super.initState();
    final s = widget.s.settings!;
    final r = s.rules;
    int mins(int? secs) => secs == null ? 0 : secs ~/ 60;
    _c = {
      'game': TextEditingController(text: '${mins(s.gameSeconds)}'),
      'turn': TextEditingController(text: '${s.turnSeconds ?? 0}'),
      'bank': TextEditingController(text: '${s.timeBankSeconds ?? 0}'),
      'starting_balance': TextEditingController(text: '${r.startingBalance}'),
      'go_salary': TextEditingController(text: '${r.goSalary}'),
      'velocity_min': TextEditingController(text: '${r.velocityMin}'),
      'velocity_max': TextEditingController(text: '${r.velocityMax}'),
      'max_houses': TextEditingController(text: '${r.maxHousesPerProperty}'),
      'bankruptcy_threshold':
          TextEditingController(text: '${r.bankruptcyThreshold}'),
      'expropriation': TextEditingController(text: '${r.expropriation}'),
      'rent_boost': TextEditingController(text: '${r.rentBoost}'),
      'win_full_groups': TextEditingController(text: '${r.winFullGroups}'),
      'win_points': TextEditingController(text: '${r.winVictoryPoints}'),
      'subsidiary_pool':
          TextEditingController(text: '${r.subsidiaryPoolFactor}'),
      'conglomerate_pool':
          TextEditingController(text: '${r.conglomeratePoolFactor}'),
      'spotlight_rent_pct':
          TextEditingController(text: '${r.spotlightRentPct}'),
      'spotlight_duration':
          TextEditingController(text: '${r.spotlightDurationTurns}'),
    };
  }

  @override
  void dispose() {
    for (final c in _c.values) {
      c.dispose();
    }
    super.dispose();
  }

  int _n(String k) => int.tryParse(_c[k]!.text.trim()) ?? 0;

  void _apply() {
    final gameMin = _n('game'), turnSec = _n('turn'), bankSec = _n('bank');
    widget.s.configure({
      'game_seconds': gameMin > 0 ? gameMin * 60 : null,
      'turn_seconds': turnSec > 0 ? turnSec : null,
      'time_bank_seconds': bankSec > 0 ? bankSec : null,
      'rules': {
        'starting_balance': _n('starting_balance'),
        'go_salary': _n('go_salary'),
        'velocity_min': _n('velocity_min'),
        'velocity_max': _n('velocity_max'),
        'max_houses_per_property': _n('max_houses'),
        'bankruptcy_threshold': _n('bankruptcy_threshold'),
        'expropriation': _n('expropriation'),
        'rent_boost': _n('rent_boost'),
        'win_full_groups': _n('win_full_groups'),
        'win_victory_points': _n('win_points'),
        'subsidiary_pool_factor': _n('subsidiary_pool'),
        'conglomerate_pool_factor': _n('conglomerate_pool'),
        'spotlight_rent_pct': _n('spotlight_rent_pct'),
        'spotlight_duration_turns': _n('spotlight_duration'),
      },
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s.settings!;
    final t = AppLocalizations.of(context);
    final host = widget.s.seat == 0;
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(bottom: 8),
        title: Text(t.settingsTitle,
            style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
        subtitle: Text(_summary(s, t),
            style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
        children: host ? _hostFields(t) : _readOnly(s, t),
      ),
    );
  }

  String _summary(RoomSettings s, AppLocalizations t) {
    final g = s.gameSeconds == null
        ? t.settingOff
        : t.settingMinutes(s.gameSeconds! ~/ 60);
    final tn =
        s.turnSeconds == null ? t.settingOff : t.settingSeconds(s.turnSeconds!);
    final b = s.timeBankSeconds == null
        ? t.settingOff
        : t.settingSeconds(s.timeBankSeconds!);
    return t.settingsSummary(g, tn, b);
  }

  List<Widget> _hostFields(AppLocalizations t) => [
        for (final key in _fieldKeys)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Row(children: [
              Expanded(
                  child: Text(_hostLabel(t, key),
                      style: const TextStyle(fontSize: 12))),
              SizedBox(
                width: 84,
                child: TextField(
                  controller: _c[key],
                  keyboardType: TextInputType.number,
                  textAlign: TextAlign.right,
                  decoration: const InputDecoration(isDense: true),
                ),
              ),
            ]),
          ),
        const SizedBox(height: 4),
        wideButton(t.settingApply, _apply, primary: false),
      ];

  List<Widget> _readOnly(RoomSettings s, AppLocalizations t) {
    final r = s.rules;
    final rows = <(String, String)>[
      (
        t.settingRoGameLength,
        s.gameSeconds == null
            ? t.settingOff
            : t.settingMinutes(s.gameSeconds! ~/ 60)
      ),
      (
        t.settingRoTurnLimit,
        s.turnSeconds == null ? t.settingOff : t.settingSeconds(s.turnSeconds!)
      ),
      (
        t.settingRoTimeBank,
        s.timeBankSeconds == null
            ? t.settingOff
            : t.settingSeconds(s.timeBankSeconds!)
      ),
      (t.settingStartingBalance, '\$${r.startingBalance}'),
      (t.settingGoSalary, '\$${r.goSalary}'),
      (t.settingRoVelocity, '${r.velocityMin}-${r.velocityMax}'),
      (t.settingRoMaxHouses, '${r.maxHousesPerProperty}'),
      (t.settingBankruptcyThreshold, '\$${r.bankruptcyThreshold}'),
      (
        t.settingRoExpropriation,
        r.expropriation == 0 ? t.settingOff : t.settingPercent(r.expropriation)
      ),
      (
        t.settingRoRentBoost,
        r.rentBoost == 0 ? t.settingOff : t.settingPercent(r.rentBoost)
      ),
      (
        t.settingRoDomination,
        r.winFullGroups == 0 ? t.settingOff : t.settingGroups(r.winFullGroups)
      ),
      (
        t.settingRoVictoryPoints,
        r.winVictoryPoints == 0 ? t.settingOff : '${r.winVictoryPoints}'
      ),
      (
        t.settingRoSubsidiaryPool,
        r.subsidiaryPoolFactor == 0
            ? t.settingOff
            : t.settingPoolFactor(r.subsidiaryPoolFactor)
      ),
      (
        t.settingRoConglomeratePool,
        r.conglomeratePoolFactor == 0
            ? t.settingOff
            : t.settingPoolFactor(r.conglomeratePoolFactor)
      ),
      (
        t.settingRoSpotlight,
        r.spotlightRentPct == 0
            ? t.settingOff
            : t.settingSpotlightValue(
                r.spotlightRentPct, r.spotlightDurationTurns)
      ),
    ];
    return [
      for (final (label, value) in rows)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 2),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(label, style: const TextStyle(fontSize: 12)),
              Text(value,
                  style: const TextStyle(
                      fontSize: 12, fontWeight: FontWeight.w600)),
            ],
          ),
        ),
    ];
  }
}

/// Post-game survey card (side panel, dismissible, one per game).
class _FeedbackCard extends StatefulWidget {
  final GameSession s;
  const _FeedbackCard({required this.s});

  @override
  State<_FeedbackCard> createState() => _FeedbackCardState();
}

class _FeedbackCardState extends State<_FeedbackCard> {
  int _rating = 0;
  final _comment = TextEditingController();

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(
              child: Text(t.feedbackTitle,
                  style: const TextStyle(
                      fontSize: 12,
                      color: Pc.textMuted,
                      letterSpacing: 1)),
            ),
            hoverSfx(IconButton(
              icon: const Icon(Icons.close, size: 16),
              onPressed: s.dismissFeedback,
              tooltip: t.feedbackDismiss,
            )),
          ]),
          Row(children: [
            for (var star = 1; star <= 5; star++)
              hoverSfx(IconButton(
                icon: Icon(
                  star <= _rating ? Icons.star : Icons.star_border,
                  color: Pc.gold,
                ),
                onPressed: () => setState(() => _rating = star),
              )),
          ]),
          TextField(
            controller: _comment,
            maxLength: 500,
            decoration: InputDecoration(
                labelText: t.feedbackCommentHint, counterText: ''),
          ),
          const SizedBox(height: 6),
          hoverSfx(FilledButton(
            onPressed: _rating == 0
                ? null
                : () => s.sendFeedback(_rating, _comment.text),
            child: Text(t.feedbackSend),
          )),
        ]),
      ),
    );
  }
}

// -- trade composer ---------------------------------------------------------------

class TradeDialog extends StatefulWidget {
  final GameSession s;
  const TradeDialog({super.key, required this.s});

  @override
  State<TradeDialog> createState() => _TradeDialogState();
}

class _TradeDialogState extends State<TradeDialog> {
  int? _to;
  final _giveCash = TextEditingController(text: '0');
  final _receiveCash = TextEditingController(text: '0');
  final _giveTiles = <String>{};
  final _receiveTiles = <String>{};

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    final v = s.view!;
    final candidates = [
      for (var i = 0; i < v.players.length; i++)
        if (i != s.seat && !v.players[i].bankrupt) i,
    ];
    _to ??= candidates.firstOrNull;

    Widget tileList(int? seat, Set<String> picked) {
      final tiles = [
        for (var i = 0; i < s.content!.board.length; i++)
          if (seat != null &&
              v.tiles[i].owner == seat &&
              s.content!.board[i].isProperty)
            i,
      ];
      return SizedBox(
        height: 140,
        width: 200,
        child: ListView(children: [
          for (final i in tiles)
            CheckboxListTile(
              dense: true,
              value: picked.contains(s.content!.board[i].id),
              title: Text(
                s.tileName(i) + (v.tiles[i].mortgaged ? ' (M)' : ''),
                style: const TextStyle(fontSize: 12),
              ),
              onChanged: (on) => setState(() {
                final id = s.content!.board[i].id;
                on == true ? picked.add(id) : picked.remove(id);
              }),
            ),
        ]),
      );
    }

    Widget cashField(TextEditingController c) => SizedBox(
          width: 200,
          child: TextField(
            controller: c,
            keyboardType: TextInputType.number,
            decoration:
                InputDecoration(labelText: t.cashLabel, isDense: true),
          ),
        );

    return AlertDialog(
      title: Text(t.tradeNewOfferTitle),
      content: Column(mainAxisSize: MainAxisSize.min, children: [
        DropdownButton<int>(
          value: _to,
          isExpanded: true,
          items: [
            for (final i in candidates)
              DropdownMenuItem(value: i, child: Text(s.playerName(i))),
          ],
          onChanged: (i) => setState(() {
            _to = i;
            _receiveTiles.clear();
          }),
        ),
        Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Column(children: [
            Text(t.tradeYouGive),
            cashField(_giveCash),
            tileList(s.seat, _giveTiles),
          ]),
          const SizedBox(width: 12),
          Column(children: [
            Text(t.tradeYouWant),
            cashField(_receiveCash),
            tileList(_to, _receiveTiles),
          ]),
        ]),
      ]),
      actions: [
        hoverSfx(TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(t.close))),
        hoverSfx(FilledButton(
          onPressed: _to == null
              ? null
              : () {
                  widget.s.sendCmd({
                    'type': 'propose_trade',
                    'to': v.players[_to!].id,
                    'give_cash': int.tryParse(_giveCash.text) ?? 0,
                    'give_tiles': _giveTiles.toList(),
                    'receive_cash': int.tryParse(_receiveCash.text) ?? 0,
                    'receive_tiles': _receiveTiles.toList(),
                  });
                  Navigator.pop(context);
                },
          child: Text(t.tradePropose),
        )),
      ],
    );
  }
}
