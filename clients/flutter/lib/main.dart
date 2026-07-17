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
import 'protocol.dart';
import 'session.dart';
import 'sfx.dart';
import 'tokens.dart';
import 'ui/game/countdown.dart';
import 'ui/game/event_log.dart';
import 'ui/game/flashes.dart';
import 'ui/game/toggles.dart';
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
              Countdown(endsAt: s.gameEndsAt!),
              const SizedBox(width: 8),
            ],
            MotionButton(s: s),
            const MuteButton(),
          ]),
        const SizedBox(height: 4),
        Row(children: [
          Expanded(
              child: Text(_status(t),
                  style: const TextStyle(fontWeight: FontWeight.w600))),
          if (s.turnEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            Countdown(
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
            Countdown(
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
            Countdown(
                endsAt: s.bidEndsAt!,
                icon: Icons.gavel,
                warnSecs: 3,
                paused: s.isAnimating),
          ],
          // Corruption bribe vote window (ADR-0024): same pattern.
          if (s.voteEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: 6),
            Countdown(
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
          Expanded(child: EventLog(log: s.log)),
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
