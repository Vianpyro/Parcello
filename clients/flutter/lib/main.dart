/// Parcello Flutter client: desktop (Windows/Linux/macOS) and web from one
/// codebase. The server stays the only authority.
library;


import 'package:flutter/foundation.dart'
    show LicenseRegistry, LicenseEntryWithLineBreaks;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'board.dart';
import 'l10n/app_localizations.dart';
import 'overlay.dart';
import 'session.dart';
import 'tokens.dart';
import 'ui/game/center_panel.dart';
import 'ui/game/flashes.dart';
import 'ui/side/side_panel.dart';
import 'ui/connect_screen.dart';
import 'ui/menu/menu_screen.dart';

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
    final centre = CenterPanel(s: s);

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
                        CardFlash(
                            seq: s.stage.cardSeq, value: s.stage.cardValue),
                        // Card reveals, spotlight and market announcements all
                        // share one banner: same shape, same place, every time.
                        // A player should never have to work out *where* the
                        // game is about to tell them something.
                        BannerFlash(
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
                  child: SingleChildScrollView(child: SidePanel(s: s)),
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
