/// Step 3: the table itself.
///
/// Note the shape of `build`: the centre panel is built ONCE here and handed
/// to the board as a `child`, so an animation frame repaints the board without
/// touching the text fields a player is typing into. Guarded by
/// test/bid_input_test.dart.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../board.dart';
import '../../l10n/app_localizations.dart';
import '../../overlay.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../reconnect_banner.dart';
import '../side/side_panel.dart';
import 'center_panel.dart';
import 'flashes.dart';
import 'market_strip.dart';
import 'nav_rail.dart';
import 'player_bar.dart';
import 'property_panel.dart';

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
              padding: const EdgeInsets.all(Pc.s12),
              child: Column(children: [
                // The seats live across the top now (DDR-0021), not in the
                // side panel. It carries the chit anchors and the bid reveal,
                // so - like the old seat list - it repaints on a stage frame.
                ListenableBuilder(
                    listenable: s.stage,
                    builder: (context, _) => PlayerBar(s: s)),
                // The public market state (pools / forecast / spotlight) that
                // used to stack in the board centre, now a thin band under the
                // bar (DDR-0021 decision); renders nothing when there is none.
                MarketStrip(s: s),
                const SizedBox(height: Pc.s4),
                Expanded(
                  child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                // Left utility rail (DDR-0021): Menu / Objectives / History.
                NavRail(s: s),
                const SizedBox(width: Pc.s8),
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
                const SizedBox(width: Pc.s12),
                // Right region (DDR-0021): the property deed pinned at the top,
                // the room-and-trades stack scrolling below it. The scroll lives
                // inside _RightColumn now - only the growing remainder moves (up
                // to four offers per proposer, the survey, the settings expander;
                // six offers already overflow a 1280x800 Steam Deck), while the
                // deed keeps its own stable slot.
                SizedBox(width: 340, child: _RightColumn(s: s)),
                      ]),
                ),
              ]),
            ),
            // Chits crossing from the board to the side panel, and the P1
            // arrest. Above everything, because money travelling from a tile to
            // a seat marker crosses both subtrees - which is exactly why a
            // board-local floater could never express the money rule.
            StageOverlay(stage: s.stage),
            // Above the overlay: when the socket is being re-established
            // (ADR-0037) the player must be able to see that the table is
            // not simply frozen. Renders nothing in the normal case.
            ReconnectBanner(s: s),
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
    // A spectator holds no seat: without this, `owner == seat` would match
    // unowned tiles (null == null) and offer a watcher owner actions.
    if (s.seat == null) return false;
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

/// The right region (DDR-0021): a pure composition widget with no logic of its
/// own beyond choosing which tile the property deed shows. It pins the deed to
/// the top and lets the room/trades stack (`SidePanel`) scroll below it - no
/// separator between the two, the split is purely structural.
class _RightColumn extends StatelessWidget {
  final GameSession s;
  const _RightColumn({required this.s});

  @override
  Widget build(BuildContext context) {
    final v = s.view;
    // The property card shows the tile under the cursor/focus, else the one the
    // player is standing on (DDR-0021 right region). Moved verbatim from
    // SidePanel: the deed now owns its own stable slot instead of scrolling
    // with the trades below it.
    final focusTile = (v != null && !v.finished)
        ? (s.hoverTile ??
              (s.seat != null
                  ? v.players.elementAtOrNull(s.seat!)?.position
                  : null))
        : null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (focusTile != null && s.content != null) ...[
          PropertyPanel(s: s, tile: focusTile),
          const SizedBox(height: Pc.s6),
        ],
        // Only this lower region scrolls - it grows with the room; the deed
        // above stays put.
        Expanded(child: SingleChildScrollView(child: SidePanel(s: s))),
      ],
    );
  }
}
