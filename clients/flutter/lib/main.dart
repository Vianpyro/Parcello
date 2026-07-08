/// Parcello Flutter client (Windows desktop first). Mirrors the embedded web
/// client feature-for-feature; the server stays the only authority.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'board.dart';
import 'oidc.dart';
import 'protocol.dart';
import 'session.dart';
import 'sfx.dart';
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
        scaffoldBackgroundColor: const Color(0xFF1C1F26),
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFFD8B45A),
          brightness: Brightness.dark,
        ),
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
                          color: Color(0xFFD8B45A))),
                  const SizedBox(height: 2),
                  const Text('Connect to a server',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: Color(0xFF9AA3B2))),
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
                      style: const TextStyle(color: Color(0xFF9AA3B2))),
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
        backgroundColor: const Color(0xFF262B35),
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
                                color: Color(0xFF9AA3B2))),
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
                                color: Color(0xFF9AA3B2))),
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
                                fontSize: 12, color: Color(0xFF9AA3B2))),
                      ],
                    ),
                  ),
                ),
                const SizedBox(height: 10),
                Text(s.loginMessage,
                    textAlign: TextAlign.center,
                    style: const TextStyle(color: Color(0xFFC0564F))),
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
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Expanded(
            child: Stack(alignment: Alignment.center, children: [
              BoardWidget(
                content: s.content!,
                view: s.view,
                mySeat: s.seat,
                onTileTap: (i) => _tileMenu(context, i),
                center: _CenterPanel(s: s),
              ),
              // Dice result, floating over the middle of the board.
              _DiceRoll(seq: s.diceSeq, d1: s.diceD1, d2: s.diceD2),
            ]),
          ),
          const SizedBox(width: 12),
          SizedBox(width: 340, child: _SidePanel(s: s)),
        ]),
      ),
    );
  }

  /// Tile actions: build/sell/boost/mortgage on my tiles (ADR-0012),
  /// expropriate a rival's raw property (ADR-0011).
  void _tileMenu(BuildContext context, int i) {
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
            !ts.mortgaged &&
            s.myTurn &&
            v.turn.type == 'await_end' &&
            v.players[s.seat!].position == i) {
          // Improved tiles liquidate on seizure - the former owner is
          // refunded half cost per level on top of the usual compensation
          // (ADR-0022).
          items.add(ListTile(
              title: Text(ts.houses > 0
                  ? 'Seize + liquidate (\$${price * expro ~/ 100})'
                  : 'Seize (\$${price * expro ~/ 100})'),
              subtitle: const Text('take this tile from its owner'),
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
    return DefaultTextStyle(
      style: const TextStyle(color: Color(0xFF2A2A2A), fontSize: 13),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [
          const Text('PARCELLO',
              style: TextStyle(
                  fontSize: 20, fontWeight: FontWeight.bold, letterSpacing: 2)),
          const Spacer(),
          // Shown for the whole game, end included: the final time left is
          // part of the result (a bankruptcy win keeps time on the clock).
          if (s.gameEndsAt != null) ...[
            _Countdown(endsAt: s.gameEndsAt!),
            const SizedBox(width: 8),
          ],
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
                warnSecs: 10),
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
                warnSecs: 10),
          ],
        ]),
        if (_poolsLine() != null) ...[
          const SizedBox(height: 2),
          Text(_poolsLine()!,
              style: const TextStyle(fontSize: 11, color: Color(0xFF9AA3B2))),
        ],
        if (_forecastLine() != null) ...[
          const SizedBox(height: 2),
          Text(_forecastLine()!,
              style: const TextStyle(fontSize: 11, color: Color(0xFF9AA3B2))),
        ],
        const SizedBox(height: 6),
        _Actions(s: s),
        const SizedBox(height: 6),
        Expanded(child: _EventLog(log: s.log)),
      ]),
    );
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
      case 'auction':
        final high = t.highBidder == null
            ? 'no bids'
            : '\$${t.highBid} by ${s.playerName(t.highBidder!)}';
        return 'Auction: ${s.tileName(t.tile!)} ($high) — '
            '${s.playerName(t.turnSeat!)} to act';
      case 'await_buy':
        final price = s.content!.board[t.tile!].price;
        return '${s.playerName(v.current)} may buy ${s.tileName(t.tile!)} for \$$price';
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
  const _Countdown(
      {required this.endsAt,
      this.icon = Icons.timer,
      this.warnSecs = 60,
      this.holdUntil});

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  // Seconds-remaining values worth a countdown cue: the final stretch plus
  // the "heads up" marks further out.
  static const _milestones = {60, 30, 10, 5, 4, 3, 2, 1, 0};

  Timer? _timer;
  int? _lastTicked;

  int _secsLeft() {
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
    if (old.endsAt != widget.endsAt) _lastTicked = null;
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
        warn ? const Color(0xFFC0564F) : const Color(0xFF2A2A2A);
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
          color: const Color(0xFF2A2A2A)),
      onPressed: () => setState(() => sfx.enabled = !sfx.enabled),
    ));
  }
}

/// The dice result, shown big in the middle of the board for a couple of
/// seconds after each roll, then faded out (like a physical board game).
class _DiceRoll extends StatefulWidget {
  final int seq, d1, d2;
  const _DiceRoll({required this.seq, required this.d1, required this.d2});

  @override
  State<_DiceRoll> createState() => _DiceRollState();
}

class _DiceRollState extends State<_DiceRoll> {
  bool _visible = false;
  Timer? _timer;

  @override
  void didUpdateWidget(_DiceRoll old) {
    super.didUpdateWidget(old);
    if (widget.seq != old.seq && widget.seq > 0) {
      setState(() => _visible = true);
      _timer?.cancel();
      _timer = Timer(const Duration(milliseconds: 2500), () {
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
        duration: const Duration(milliseconds: 300),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          _Die(widget.d1),
          const SizedBox(width: 18),
          _Die(widget.d2),
        ]),
      ),
    );
  }
}

/// A single pip die face (1-6).
class _Die extends StatelessWidget {
  final int value;
  const _Die(this.value);

  // Lit cells of a 3x3 grid (row*3 + col) per standard pip layout.
  static const _pips = <int, List<int>>{
    1: [4],
    2: [0, 8],
    3: [0, 4, 8],
    4: [0, 2, 6, 8],
    5: [0, 2, 4, 6, 8],
    6: [0, 2, 3, 5, 6, 8],
  };

  @override
  Widget build(BuildContext context) {
    final on = (_pips[value] ?? const <int>[]).toSet();
    return Container(
      width: 66,
      height: 66,
      padding: const EdgeInsets.all(9),
      decoration: BoxDecoration(
        color: Colors.white,
        borderRadius: BorderRadius.circular(12),
        boxShadow: const [
          BoxShadow(color: Colors.black54, blurRadius: 10, offset: Offset(0, 4)),
        ],
      ),
      child: Column(
        children: [
          for (var r = 0; r < 3; r++)
            Expanded(
              child: Row(children: [
                for (var c = 0; c < 3; c++)
                  Expanded(
                    child: Center(
                      child: on.contains(r * 3 + c)
                          ? Container(
                              width: 12,
                              height: 12,
                              decoration: const BoxDecoration(
                                color: Color(0xFF1E1E1E),
                                shape: BoxShape.circle,
                              ),
                            )
                          : const SizedBox.shrink(),
                    ),
                  ),
              ]),
            ),
        ],
      ),
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

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final v = s.view;
    if (v == null || v.finished) return const SizedBox.shrink();
    final t = v.turn;

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
    if (t.type == 'auction') {
      if (t.turnSeat != s.seat) return const SizedBox.shrink();
      _bid.text = '${t.highBid + 1}';
      children.addAll([
        SizedBox(
          width: 90,
          child: TextField(
            controller: _bid,
            keyboardType: TextInputType.number,
            style: const TextStyle(color: Color(0xFF2A2A2A)),
            decoration: const InputDecoration(isDense: true),
          ),
        ),
        hoverSfx(FilledButton(
          onPressed: () => s
              .sendCmd({'type': 'bid', 'amount': int.tryParse(_bid.text) ?? 0}),
          child: const Text('Bid'),
        )),
        btn('Pass', {'type': 'pass'}, primary: false),
      ]);
    } else if (s.myTurn) {
      final me = v.players[s.seat!];
      switch (t.type) {
        case 'await_roll':
          children.add(btn('Roll', {'type': 'roll'}));
          if (me.inJail) {
            children
                .add(btn('Pay fine', {'type': 'pay_jail_fine'}, primary: false));
            if (me.jailCards > 0) {
              children.add(btn('Use jail card', {'type': 'use_jail_card'},
                  primary: false));
            }
          }
        case 'await_buy':
          final price = s.content!.board[t.tile!].price;
          children.add(btn('Buy (\$$price)', {'type': 'buy'}));
          children.add(btn('Decline', {'type': 'decline'}, primary: false));
        case 'await_end':
          children.add(btn('End turn', {'type': 'end_turn'}));
      }
      children.add(const Text('Tap your tiles to build / mortgage.',
          style: TextStyle(color: Color(0xFF777777), fontSize: 11)));
    }
    return Wrap(
        spacing: 6,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: children);
  }
}

class _EventLog extends StatelessWidget {
  final List<String> log;
  const _EventLog({required this.log});

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFDF6),
        border: Border.all(color: const Color(0xFFC9C4AE)),
        borderRadius: BorderRadius.circular(4),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: ListView.builder(
        reverse: true, // newest visible without scroll management
        itemCount: log.length,
        itemBuilder: (ctx, i) => Text(
          log[log.length - 1 - i],
          style: const TextStyle(fontSize: 11, color: Color(0xFF333333)),
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
          color: const Color(0xFF2E2A1C),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('${s.playerName(v.winner!)} wins!',
                      style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: Color(0xFFD8B45A))),
                  const SizedBox(height: 8),
                  Row(children: [
                    Expanded(child: wideButton('Play again', s.sendPlayAgain)),
                    const SizedBox(width: 8),
                    Expanded(
                        child: wideButton('Continue', s.leaveRoom,
                            primary: false)),
                  ]),
                  const Text('"Play again" restarts for everyone still here.',
                      style: TextStyle(fontSize: 11, color: Color(0xFF9AA3B2))),
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
                        color: Color(0xFFD8B45A),
                        letterSpacing: 2)),
              ),
              if (s.code != null)
                hoverSfx(IconButton(
                  iconSize: 18,
                  visualDensity: VisualDensity.compact,
                  tooltip: 'Copy room code',
                  icon: const Icon(Icons.copy, color: Color(0xFF9AA3B2)),
                  onPressed: () => copyCode(context, s.code!),
                )),
            ]),
            const SizedBox(height: 6),
            _players(),
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
                foregroundColor: const Color(0xFFC0564F)),
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

  Widget _players() {
    final v = s.view;
    final rows = <Widget>[];
    final count = v?.players.length ?? s.seats.length;
    for (var i = 0; i < count; i++) {
      final p = v?.players.elementAtOrNull(i);
      final seatInfo = s.seats.elementAtOrNull(i);
      final name = p?.name ?? seatInfo?.name ?? 'seat $i';
      final tags = [
        if (i == s.seat) '(you)',
        if (p?.inJail == true) '[jail]',
        if ((p?.jailCards ?? 0) > 0) '[${p!.jailCards} jail card]',
        if (seatInfo?.isBot == true)
          '\u{1F916} bot'
        else if (seatInfo?.connected == false)
          '(offline)',
      ].join(' ');
      rows.add(Opacity(
        opacity: p?.bankrupt == true ? 0.4 : 1,
        child: Row(children: [
          Container(
            width: 12,
            height: 12,
            decoration: BoxDecoration(
                color: pawnColors[i % pawnColors.length],
                shape: BoxShape.circle),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text('$name $tags',
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight:
                      v != null && v.current == i ? FontWeight.bold : null,
                  decoration:
                      p?.bankrupt == true ? TextDecoration.lineThrough : null,
                )),
          ),
          if (p != null)
            Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
              Text('\$${p.cash}'),
              // Net worth decides a timed game (ADR-0010), so surface it then.
              if (s.gameEndsAt != null)
                Text('NW \$${s.netWorth(i)}',
                    style: const TextStyle(
                        fontSize: 11, color: Color(0xFF9AA3B2))),
            ]),
        ]),
      ));
    }
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
              fontSize: 12, color: Color(0xFF9AA3B2), letterSpacing: 1)),
      const SizedBox(height: 6),
      if (offers.isEmpty)
        const Text('No open offers.',
            style: TextStyle(color: Color(0xFF9AA3B2))),
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
    ('jail_fine', 'Jail fine'),
    ('max_houses', 'Max houses (1-5)'),
    ('bankruptcy_threshold', 'Bankruptcy threshold'),
    ('expropriation', 'Expropriation %'),
    ('rent_boost', 'Rent boost %'),
    ('win_full_groups', 'Domination groups (0=off)'),
    ('subsidiary_pool', 'Subsidiary pool factor (0=off)'),
    ('conglomerate_pool', 'Conglomerate pool factor (0=off)'),
  ];
  late final Map<String, TextEditingController> _c;
  late bool _auction;

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
      'jail_fine': TextEditingController(text: '${r.jailFine}'),
      'max_houses': TextEditingController(text: '${r.maxHousesPerProperty}'),
      'bankruptcy_threshold':
          TextEditingController(text: '${r.bankruptcyThreshold}'),
      'expropriation': TextEditingController(text: '${r.expropriation}'),
      'rent_boost': TextEditingController(text: '${r.rentBoost}'),
      'win_full_groups': TextEditingController(text: '${r.winFullGroups}'),
      'subsidiary_pool':
          TextEditingController(text: '${r.subsidiaryPoolFactor}'),
      'conglomerate_pool':
          TextEditingController(text: '${r.conglomeratePoolFactor}'),
    };
    _auction = r.auctionOnDecline;
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
        'jail_fine': _n('jail_fine'),
        'max_houses_per_property': _n('max_houses'),
        'bankruptcy_threshold': _n('bankruptcy_threshold'),
        'auction_on_decline': _auction,
        'expropriation': _n('expropriation'),
        'rent_boost': _n('rent_boost'),
        'win_full_groups': _n('win_full_groups'),
        'subsidiary_pool_factor': _n('subsidiary_pool'),
        'conglomerate_pool_factor': _n('conglomerate_pool'),
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
            style: const TextStyle(fontSize: 11, color: Color(0xFF9AA3B2))),
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
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          dense: true,
          title: const Text('Auction on decline',
              style: TextStyle(fontSize: 12)),
          value: _auction,
          onChanged: (v) {
            v ? sfx.toggleOn() : sfx.toggleOff();
            setState(() => _auction = v);
          },
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
      ('Jail fine', '\$${r.jailFine}'),
      ('Max houses', '${r.maxHousesPerProperty}'),
      ('Bankruptcy threshold', '\$${r.bankruptcyThreshold}'),
      ('Auctions', r.auctionOnDecline ? 'on' : 'off'),
      ('Expropriation', r.expropriation == 0 ? 'off' : '${r.expropriation}%'),
      ('Rent boost', r.rentBoost == 0 ? 'off' : '${r.rentBoost}%'),
      ('Domination', r.winFullGroups == 0 ? 'off' : '${r.winFullGroups} groups'),
      (
        'Subsidiary pool',
        r.subsidiaryPoolFactor == 0 ? 'off' : 'x${r.subsidiaryPoolFactor}'
      ),
      (
        'Conglomerate pool',
        r.conglomeratePoolFactor == 0 ? 'off' : 'x${r.conglomeratePoolFactor}'
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
                      color: Color(0xFF9AA3B2),
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
                  color: const Color(0xFFD8B45A),
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
