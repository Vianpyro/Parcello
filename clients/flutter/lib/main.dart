/// Parcello Flutter client: desktop (Windows/Linux/macOS) and web from one
/// codebase. The server stays the only authority.
library;

import 'dart:async';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'board.dart';
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

void main() => runApp(ParcelloApp(session: GameSession()));

class ParcelloApp extends StatelessWidget {
  final GameSession session;
  const ParcelloApp({super.key, required this.session});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Parcello',
      theme: ThemeData(
        brightness: Brightness.dark,
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
    content: Text('Room code $code copied'),
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
    final issuer = TextEditingController(
        text: s.savedIssuer.isEmpty ? 'https://' : s.savedIssuer);
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Sign in'),
        content: TextField(
          controller: issuer,
          decoration: const InputDecoration(
              labelText: 'Identity provider URL',
              hintText: 'https://auth.example.com'),
        ),
        actions: [
          hoverSfx(TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'))),
          hoverSfx(FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Open browser'))),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    try {
      s.saveIssuer(issuer.text.trim());
      final token = await loginWithOidc(issuer.text.trim(), 'parcello');
      setState(() {
        _token.text = token;
        _signedInAs = jwtDisplayName(token) ?? 'account';
      });
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Sign-in failed: $e')));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
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
                  const Text('Parcello',
                      textAlign: TextAlign.center,
                      style: TextStyle(
                          fontSize: 30,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: 2),
                  const Text('Connect to a server',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: Pc.textMuted)),
                  const SizedBox(height: 16),
                  TextField(
                    controller: _url,
                    decoration: const InputDecoration(labelText: 'Server URL'),
                  ),
                  TextField(
                    controller: _name,
                    maxLength: 24,
                    decoration:
                        const InputDecoration(labelText: 'Display name'),
                  ),
                  const SizedBox(height: 8),
                  wideButton(
                      _signedInAs == null
                          ? 'Sign in with account (optional)'
                          : 'Signed in as $_signedInAs',
                      _signIn,
                      primary: false),
                  const SizedBox(height: 10),
                  wideButton('Connect', () {
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

/// Step 2 (connected): create a private game, join one by code, or (soon)
/// browse public games.
class MenuScreen extends StatefulWidget {
  final GameSession s;
  const MenuScreen({super.key, required this.s});

  @override
  State<MenuScreen> createState() => _MenuScreenState();
}

class _MenuScreenState extends State<MenuScreen> {
  final _code = TextEditingController();
  final _mods = TextEditingController();

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Parcello'),
        backgroundColor: Pc.surface2,
        actions: [
          TextButton.icon(
            onPressed: s.disconnectFromServer,
            icon: const Icon(Icons.logout, size: 18),
            label: const Text('Disconnect'),
          ),
        ],
      ),
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(16),
          child: SizedBox(
            width: 420,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const Text('PRIVATE GAME',
                            style: TextStyle(
                                fontSize: 12,
                                letterSpacing: 1,
                                color: Pc.textMuted)),
                        const SizedBox(height: 10),
                        wideButton('Create a game',
                            () => s.createGame(mods: _parseMods())),
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 6),
                          child: TextField(
                            controller: _mods,
                            decoration: const InputDecoration(
                                isDense: true,
                                labelText: 'Mods (optional, comma-separated)'),
                          ),
                        ),
                        const Divider(height: 24),
                        TextField(
                          controller: _code,
                          maxLength: 5,
                          textCapitalization: TextCapitalization.characters,
                          decoration:
                              const InputDecoration(labelText: 'Room code'),
                        ),
                        wideButton('Join by code', () {
                          if (_code.text.trim().isEmpty) return;
                          s.joinGame(_code.text);
                        }, primary: false),
                      ],
                    ),
                  ),
                ),
                // LAN discovery and local server management have no
                // browser equivalent (no raw sockets, no process spawn in
                // a sandbox) - hide the whole card on the web build rather
                // than shipping dead-end buttons.
                if (!kIsWeb) ...[
                  const SizedBox(height: 12),
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const Text('PUBLIC GAMES',
                              style: TextStyle(
                                  fontSize: 12,
                                  letterSpacing: 1,
                                  color: Pc.textMuted)),
                          const SizedBox(height: 10),
                          wideButton('Browse public games', () {
                            Navigator.push(
                              context,
                              MaterialPageRoute(
                                builder: (_) => LanBrowser(session: s)));
                          }),
                          const SizedBox(height: 6),
                          wideButton('Server Manager', () {
                            Navigator.push(
                              context,
                              MaterialPageRoute(
                                builder: (_) => const ServerManager()));
                          }, primary: false),
                          const SizedBox(height: 6),
                          const Text('Coming soon.',
                              style: TextStyle(
                                  fontSize: 12, color: Pc.textMuted)),
                        ],
                      ),
                    ),
                  ),
                ],
                const SizedBox(height: 10),
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

  List<String> _parseMods() => _mods.text
      .split(',')
      .map((m) => m.trim())
      .where((m) => m.isNotEmpty)
      .toList();
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
                SizedBox(width: 340, child: _SidePanel(s: s)),
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
                title: Text('Build house (\$${def.houseCost})'),
                onTap: () {
                  s.sendCmd({'type': 'build', 'tile': def.id});
                  close();
                }));
          }
          if (ts.houses > 0) {
            items.add(ListTile(
                title: const Text('Sell house'),
                onTap: () {
                  s.sendCmd({'type': 'sell_house', 'tile': def.id});
                  close();
                }));
          }
          if (boost > 0 && !ts.mortgaged && ts.boosts < 3) {
            items.add(ListTile(
                title: Text('Boost rent (\$${price * boost ~/ 100})'),
                onTap: () {
                  s.sendCmd({'type': 'boost_rent', 'tile': def.id});
                  close();
                }));
          }
          items.add(ListTile(
              title: Text(ts.mortgaged ? 'Redeem mortgage' : 'Mortgage'),
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
            label = 'Buy out mortgage (\$${price ~/ 2})';
            subtitle = 'take this tile - stays mortgaged, redeem it after';
          } else if (ts.houses > 0) {
            label = 'Seize + liquidate (\$${price * expro ~/ 100})';
            subtitle = 'take this tile from its owner';
          } else {
            label = 'Seize (\$${price * expro ~/ 100})';
            subtitle = 'take this tile from its owner';
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
            const Text('PARCELLO',
                style: TextStyle(
                    fontSize: 20,
                    fontWeight: FontWeight.bold,
                    letterSpacing: 3,
                    color: Pc.gold)),
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
              child: Text(_status(),
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
          if (_poolsLine() != null) ...[
            const SizedBox(height: 2),
            Text(_poolsLine()!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_forecastLine() != null) ...[
            const SizedBox(height: 2),
            Text(_forecastLine()!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_spotlightLine() != null) ...[
            const SizedBox(height: 2),
            Text(_spotlightLine()!,
                style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
          ],
          if (_vpLegend() != null) ...[
            const SizedBox(height: 6),
            _vpLegend()!,
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
  Widget? _vpLegend() {
    final target = s.content?.winVictoryPoints ?? 0;
    if (s.view == null || target <= 0) return null;
    const rows = [
      ('3', 'a complete colour group'),
      ('2', 'a maxed (conglomerate) tile'),
      ('1', 'a utility tile you own'),
      ('+2', 'each round, to the richest player'),
    ];
    return Container(
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: Pc.gold.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: Pc.gold, width: 1),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('VICTORY POINTS  ·  first to $target wins',
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
        ..._roundProgress(),
      ]),
    );
  }

  /// Live state of the round metronome (ADR-0020), so the `+2` above stops
  /// looking like it arrives out of nowhere: a "round" completes when every
  /// surviving player has cycled a full hand of movement cards, and the
  /// bonus banks to whoever is richest at that instant. The round number is
  /// the MINIMUM hands-cycled across survivors - so progress is simply how
  /// many players have already pulled ahead of that minimum.
  List<Widget> _roundProgress() {
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
        Text('ROUND ${round + 1}',
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
        Text('${done.length}/${alive.length} hands cycled',
            style: const TextStyle(fontSize: 10, color: Pc.textFaint)),
      ]),
      const SizedBox(height: 2),
      Text(
        '+2 VP to ${s.playerName(leader)} (richest) when the round closes',
        style: const TextStyle(fontSize: 10, color: Pc.textMuted),
      ),
    ];
  }

  /// Shared building pools (ADR-0019): "the tension only works if everyone
  /// watches the shelf empty." Null when pooling is off entirely.
  String? _poolsLine() {
    final v = s.view;
    if (v == null) return null;
    final subs = v.subsidiariesAvailable;
    final congs = v.conglomeratesAvailable;
    if (subs == null && congs == null) return null;
    return 'Subsidiaries: ${subs ?? 'unlimited'} | Conglomerates: ${congs ?? 'unlimited'}';
  }

  /// Public market forecast (ADR-0021): reveals draws already made, not the
  /// generator. Null when nothing is scheduled or active.
  String? _forecastLine() {
    final v = s.view;
    final c = s.content;
    if (v == null || c == null) return null;
    final f = v.forecast;
    if (f.active == null && f.queue.isEmpty) return null;
    final parts = <String>[];
    if (f.active != null) {
      final a = f.active!;
      final sign = a.magnitudePct > 0 ? '+' : '';
      parts.add(
          '${c.marketEventName(a.eventId)} active ($sign${a.magnitudePct}%, ends turn ${a.endsAtTurn})');
    }
    if (f.queue.isNotEmpty) {
      final upcoming = f.queue
          .map((e) => '${c.marketEventName(e.eventId)} (turn ${e.startsAtTurn})')
          .join(', ');
      parts.add('upcoming: $upcoming');
    }
    return parts.join(' | ');
  }

  /// The Exposition corner's spotlight (ADR-0026): fully public, no per-seat
  /// masking. Null when nothing is currently spotlit.
  String? _spotlightLine() {
    final v = s.view;
    final c = s.content;
    final sp = v?.spotlight;
    if (v == null || c == null || sp == null) return null;
    // Prefer the live room rules (host may have tweaked them, ADR-0015);
    // fall back to the content snapshot from join. A permanent spotlight
    // carries u32::MAX as its expiry sentinel - don't print that.
    final pct = s.settings?.rules.spotlightRentPct ?? c.spotlightRentPct;
    final until = sp.expiresAtTurn >= 0xFFFFFFFF
        ? 'until replaced'
        : 'ends turn ${sp.expiresAtTurn}';
    return '${c.board[sp.tile].name} spotlighted (+$pct%, $until)';
  }

  String _status() {
    final v = s.view;
    if (v == null) {
      return s.seats.length >= 2
          ? 'Ready — host can start.'
          : 'Waiting for players…';
    }
    if (v.finished) return 'Game over — ${s.playerName(v.winner!)} wins!';
    final t = v.turn;
    switch (t.type) {
      case 'blind_auction':
        final pending = <int>[
          for (var i = 0; i < t.bids.length; i++)
            if (t.bids[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? 'nobody'
            : pending.map(s.playerName).join(', ');
        return 'Sealed bid on ${s.tileName(t.tile!)} — waiting on: $waiting';
      case 'bribe_vote':
        final pending = <int>[
          for (var i = 0; i < t.votes.length; i++)
            if (i != t.briber && t.votes[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? 'nobody'
            : pending.map(s.playerName).join(', ');
        return '${s.playerName(t.briber!)} offers \$${t.amount} to leave jail — waiting on: $waiting';
      default:
        return "${s.playerName(v.current)}'s turn";
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
      tooltip: sfx.enabled ? 'Mute sound' : 'Unmute sound',
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
      final price = s.content!.board[t.tile!].price ?? 0;
      final cash = v.players[seat].cash;
      final isDiscoverer = v.current == seat;
      if (_bidInitTile != t.tile) {
        // Seed at the list price, but never above what you can actually
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
              ? 'Sealed bid on ${s.tileName(t.tile!)} (floor \$$price if you stay silent):'
              : 'Sealed bid on ${s.tileName(t.tile!)}:',
          style: const TextStyle(fontSize: 12),
        ),
        // The discoverer's edge (ADR-0018): landing there took the risk,
        // so a contested win above the floor is rewarded with a discount.
        if (isDiscoverer)
          const Text(
            'Outbid a rival above the floor and you pay only 90% of your '
            'bid - your reward for landing here.',
            style: TextStyle(fontSize: 10, color: Pc.textFaint),
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
          child: const Text('Bid'),
        )),
        btn('Abstain', {'type': 'submit_blind_bid', 'amount': 0},
            primary: false),
        // Quick raises as a percent of the list price, so escalating a bid
        // doesn't mean typing out full numbers under the clock. Mutating
        // the controller already repaints the TextField bound to it - no
        // setState needed (and one less rebuild to guard against).
        for (final pct in [10, 25, 50, 100])
          hoverSfx(OutlinedButton(
            onPressed: () => bumpBid(pct),
            style: touch,
            child: Text('+$pct%'),
          )),
        // All-in: the highest bid the sealed-bid invariant will accept.
        hoverSfx(OutlinedButton(
          onPressed: () => _bid.text = '$cash',
          style: touch,
          child: Text('Max (\$$cash)'),
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
          '${s.playerName(t.briber!)} offers \$${t.amount} to leave jail:',
          style: const TextStyle(fontSize: 12),
        ),
        btn('Accept', {'type': 'vote_on_bribe', 'accept': true}),
        btn('Reject', {'type': 'vote_on_bribe', 'accept': false},
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
              child: btn('Play ${route.first} (route)',
                  {'type': 'play_movement_card', 'value': route.first}),
            ));
          } else if (me.inJail) {
            // Three exits: jail card, Corruption bribe, Legal Route.
            if (me.jailCards > 0) {
              children.add(btn('Use jail card', {'type': 'use_jail_card'},
                  primary: false));
            }
            final sorted = [...me.hand]..sort();
            if (!_bribeSeeded) {
              // No suggested-amount cap (2026-07): the engine allows
              // 1..=cash, so seed the full ceiling and let them dial down.
              _bribe.text = '${me.cash > 0 ? me.cash : 1}';
              _bribeSeeded = true;
            }
            final routeComplete = _routeOrder.length == sorted.length;
            children.addAll([
              const Text('Tap your cards in the order you want to play them:',
                  style: TextStyle(fontSize: 12)),
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
                  child: const Text('Choose route'),
                )),
                if (_routeOrder.isNotEmpty) ...[
                  const SizedBox(width: 6),
                  hoverSfx(TextButton(
                    onPressed: () => setState(() => _routeOrder.clear()),
                    child: const Text('Reset'),
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
                  'Offer bribe',
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
          children.add(btn('End turn', {'type': 'end_turn'}));
      }
      children.add(const Text('Tap your tiles to build / mortgage.',
          style: TextStyle(color: Pc.textFaint, fontSize: 11)));
    }
    return Wrap(
        spacing: 6,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: children);
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
                  Text('${s.playerName(v.winner!)} wins!',
                      style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: 8),
                  Row(children: [
                    Expanded(child: wideButton('Play again', s.sendPlayAgain)),
                    const SizedBox(width: 8),
                    Expanded(
                        child: wideButton('Continue', s.leaveRoom,
                            primary: false)),
                  ]),
                  const Text('"Play again" restarts for everyone still here.',
                      style: TextStyle(fontSize: 11, color: Pc.textMuted)),
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
                child: Text('ROOM ${s.code ?? ""}',
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
                  tooltip: 'Copy room code',
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
                listenable: s.stage, builder: (context, _) => _players()),
            if (s.view == null) ...[
              const SizedBox(height: 8),
              wideButton('Start game',
                  s.seat == 0 && s.seats.length >= 2 ? s.sendStart : null),
              // Host-only bot controls. Bots fill empty seats but yield to
              // humans, so they never block a join (ADR-0014).
              if (s.seat == 0)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Row(children: [
                    Expanded(
                        child: wideButton('Add bot',
                            s.seats.length < 6 ? s.addBot : null,
                            primary: false)),
                    const SizedBox(width: 6),
                    Expanded(
                        child: wideButton('Remove bot',
                            s.seats.any((x) => x.isBot) ? s.removeBot : null,
                            primary: false)),
                  ]),
                ),
              if (s.code != null)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: wideButton('Copy code to share', () => copyCode(context, s.code!),
                      primary: false),
                ),
              if (s.settings != null) _SettingsPanel(s: s),
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
                  title: const Text('Resign from the game?'),
                  actions: [
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonNo();
                          Navigator.pop(ctx, false);
                        },
                        child: const Text('Cancel'))),
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonYes();
                          Navigator.pop(ctx, true);
                        },
                        child: const Text('Resign'))),
                  ],
                ),
              );
              if (ok == true) s.sendCmd({'type': 'resign'});
            },
            child: const Text('Resign'),
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

  Widget _players() {
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
      final name = p?.name ?? seatInfo?.name ?? 'seat $i';
      // Whose turn is it: bold text alone read as too subtle in playtests
      // (2026-07) - a highlighted row + a leading marker reads at a glance.
      final isActive = v != null && !v.finished && v.current == i;
      final rank = ranks[i];
      final cycled =
          round != null && p != null && !p.bankrupt && p.handsCycled > round;
      final tags = [
        if (cycled) '✓ hand cycled',
        if (i == s.seat) '(you)',
        if (p?.inJail == true) '[jail]',
        if (p?.jailRoute != null) '[route: ${p!.jailRoute!.join(',')} left]',
        if ((p?.jailCards ?? 0) > 0) '[${p!.jailCards} jail card]',
        if (seatInfo?.isBot == true)
          '\u{1F916} bot'
        else if (seatInfo?.connected == false)
          '(offline)',
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
                  Text('NW \$${s.netWorth(i)}',
                      style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
                // Victory-point race (ADR-0020): "the race IS the game".
                if ((s.content?.winVictoryPoints ?? 0) > 0)
                  Text('VP ${p.victoryPoints}/${s.content!.winVictoryPoints}',
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
    final offers = v?.pendingTrades ?? [];
    String side(int cash, List<int> tiles) {
      final parts = [
        if (cash > 0) '\$$cash',
        ...tiles.map(s.tileName),
      ];
      return parts.isEmpty ? 'nothing' : parts.join(' + ');
    }

    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const Text('TRADES',
          style: TextStyle(
              fontSize: 12, color: Pc.textMuted, letterSpacing: 1)),
      const SizedBox(height: 6),
      if (offers.isEmpty)
        const Text('No open offers.',
            style: TextStyle(color: Pc.textMuted)),
      for (final o in offers)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('#${o.id} ${s.playerName(o.from)} gives '
                '${side(o.giveCash, o.giveTiles)} for '
                '${side(o.receiveCash, o.receiveTiles)} '
                '(to ${s.playerName(o.to)})'),
            Row(children: [
              if (o.to == s.seat) ...[
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'accept_trade', 'trade': o.id}),
                    child: const Text('Accept'))),
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'decline_trade', 'trade': o.id}),
                    child: const Text('Refuse'))),
              ],
              if (o.from == s.seat)
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'cancel_trade', 'trade': o.id}),
                    child: const Text('Cancel'))),
            ]),
          ]),
        ),
      if (v != null && !v.finished)
        hoverSfx(OutlinedButton(
          onPressed: () => showDialog<void>(
              context: context, builder: (ctx) => TradeDialog(s: s)),
          child: const Text('New offer'),
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
  // key -> (label, controller). Order defines the display order.
  static const _fields = [
    ('game', 'Game length (min, 0=off)'),
    ('turn', 'Turn limit (s, 0=off)'),
    ('bank', 'Time bank (s, 0=off)'),
    ('starting_balance', 'Starting balance'),
    ('go_salary', 'GO salary'),
    ('velocity_min', 'Velocity min'),
    ('velocity_max', 'Velocity max'),
    ('max_houses', 'Max houses (1-5)'),
    ('bankruptcy_threshold', 'Bankruptcy threshold'),
    ('expropriation', 'Expropriation %'),
    ('rent_boost', 'Rent boost %'),
    ('win_full_groups', 'Domination groups (0=off)'),
    ('win_points', 'Victory points target (0=off)'),
    ('subsidiary_pool', 'Subsidiary pool factor (0=off)'),
    ('conglomerate_pool', 'Conglomerate pool factor (0=off)'),
    ('spotlight_rent_pct', 'Spotlight rent % (0=off)'),
    ('spotlight_duration', 'Spotlight duration (turns)'),
  ];
  late final Map<String, TextEditingController> _c;

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
    final host = widget.s.seat == 0;
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(bottom: 8),
        title: const Text('Game settings',
            style: TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
        subtitle: Text(_summary(s),
            style: const TextStyle(fontSize: 11, color: Pc.textMuted)),
        children: host ? _hostFields() : _readOnly(s),
      ),
    );
  }

  String _summary(RoomSettings s) {
    final g = s.gameSeconds == null ? 'off' : '${s.gameSeconds! ~/ 60}min';
    final t = s.turnSeconds == null ? 'off' : '${s.turnSeconds}s';
    final b = s.timeBankSeconds == null ? 'off' : '${s.timeBankSeconds}s';
    return 'game $g - turn $t - bank $b';
  }

  List<Widget> _hostFields() => [
        for (final (key, label) in _fields)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Row(children: [
              Expanded(child: Text(label, style: const TextStyle(fontSize: 12))),
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
        wideButton('Apply settings', _apply, primary: false),
      ];

  List<Widget> _readOnly(RoomSettings s) {
    final r = s.rules;
    final rows = <(String, String)>[
      ('Game length', s.gameSeconds == null ? 'off' : '${s.gameSeconds! ~/ 60} min'),
      ('Turn limit', s.turnSeconds == null ? 'off' : '${s.turnSeconds} s'),
      ('Time bank', s.timeBankSeconds == null ? 'off' : '${s.timeBankSeconds} s'),
      ('Starting balance', '\$${r.startingBalance}'),
      ('GO salary', '\$${r.goSalary}'),
      ('Velocity', '${r.velocityMin}-${r.velocityMax}'),
      ('Max houses', '${r.maxHousesPerProperty}'),
      ('Bankruptcy threshold', '\$${r.bankruptcyThreshold}'),
      ('Expropriation', r.expropriation == 0 ? 'off' : '${r.expropriation}%'),
      ('Rent boost', r.rentBoost == 0 ? 'off' : '${r.rentBoost}%'),
      ('Domination', r.winFullGroups == 0 ? 'off' : '${r.winFullGroups} groups'),
      (
        'Victory points',
        r.winVictoryPoints == 0 ? 'off' : '${r.winVictoryPoints}'
      ),
      (
        'Subsidiary pool',
        r.subsidiaryPoolFactor == 0 ? 'off' : 'x${r.subsidiaryPoolFactor}'
      ),
      (
        'Conglomerate pool',
        r.conglomeratePoolFactor == 0 ? 'off' : 'x${r.conglomeratePoolFactor}'
      ),
      (
        'Spotlight',
        r.spotlightRentPct == 0
            ? 'off'
            : '+${r.spotlightRentPct}% / ${r.spotlightDurationTurns} turns'
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
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            const Expanded(
              child: Text('HOW WAS THE GAME?',
                  style: TextStyle(
                      fontSize: 12,
                      color: Pc.textMuted,
                      letterSpacing: 1)),
            ),
            hoverSfx(IconButton(
              icon: const Icon(Icons.close, size: 16),
              onPressed: s.dismissFeedback,
              tooltip: 'Dismiss',
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
            decoration: const InputDecoration(
                labelText: 'Anything to add? (optional)', counterText: ''),
          ),
          const SizedBox(height: 6),
          hoverSfx(FilledButton(
            onPressed: _rating == 0
                ? null
                : () => s.sendFeedback(_rating, _comment.text),
            child: const Text('Send'),
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
                const InputDecoration(labelText: 'Cash', isDense: true),
          ),
        );

    return AlertDialog(
      title: const Text('New trade offer'),
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
            const Text('You give'),
            cashField(_giveCash),
            tileList(s.seat, _giveTiles),
          ]),
          const SizedBox(width: 12),
          Column(children: [
            const Text('You want'),
            cashField(_receiveCash),
            tileList(_to, _receiveTiles),
          ]),
        ]),
      ]),
      actions: [
        hoverSfx(TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'))),
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
          child: const Text('Propose'),
        )),
      ],
    );
  }
}
