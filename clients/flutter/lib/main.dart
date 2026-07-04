/// Parcello Flutter client (Windows desktop first). Mirrors the embedded web
/// client feature-for-feature; the server stays the only authority.
library;

import 'dart:async';

import 'package:flutter/material.dart';

import 'board.dart';
import 'oidc.dart';
import 'session.dart';
import 'sfx.dart';

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
        builder: (context, _) =>
            session.joined ? GameScreen(s: session) : LoginScreen(s: session),
      ),
    );
  }
}

// -- login ---------------------------------------------------------------------

class LoginScreen extends StatefulWidget {
  final GameSession s;
  const LoginScreen({super.key, required this.s});

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _url = TextEditingController(text: 'ws://127.0.0.1:7878/ws');
  final _name = TextEditingController();
  final _code = TextEditingController();
  final _mods = TextEditingController();
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
          TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel')),
          FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Open browser')),
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
    // Rejoin hint: prefill the last room code after a disconnect.
    if (_code.text.isEmpty && s.code != null) _code.text = s.code!;
    return Scaffold(
      body: Center(
        child: Card(
          child: Container(
            width: 360,
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const Text('Parcello',
                    style: TextStyle(
                        fontSize: 28,
                        fontWeight: FontWeight.bold,
                        color: Color(0xFFD8B45A))),
                const SizedBox(height: 16),
                TextField(
                  controller: _url,
                  decoration: const InputDecoration(labelText: 'Server URL'),
                ),
                TextField(
                  controller: _name,
                  maxLength: 24,
                  decoration: const InputDecoration(labelText: 'Display name'),
                ),
                TextField(
                  controller: _code,
                  maxLength: 5,
                  textCapitalization: TextCapitalization.characters,
                  decoration: const InputDecoration(
                      labelText: 'Room code (leave empty to create)'),
                ),
                TextField(
                  controller: _mods,
                  decoration: const InputDecoration(
                      labelText: 'Mods, comma-separated (create only)'),
                ),
                TextField(
                  controller: _token,
                  obscureText: true,
                  decoration: const InputDecoration(
                      labelText: 'Identity token (optional)'),
                ),
                const SizedBox(height: 8),
                OutlinedButton(
                  onPressed: _signIn,
                  child: Text(_signedInAs == null
                      ? 'Sign in with account'
                      : 'Signed in as $_signedInAs'),
                ),
                const SizedBox(height: 12),
                FilledButton(
                  onPressed: () {
                    if (_name.text.trim().isEmpty && _token.text.trim().isEmpty) {
                      return;
                    }
                    final mods = _mods.text
                        .split(',')
                        .map((m) => m.trim())
                        .where((m) => m.isNotEmpty)
                        .toList();
                    s.connect(_url.text.trim(), _name.text.trim(),
                        _code.text.trim().toUpperCase(),
                        mods: mods, token: _token.text.trim());
                  },
                  child: const Text('Play'),
                ),
                const SizedBox(height: 8),
                Text(s.loginMessage,
                    style: const TextStyle(color: Color(0xFF9AA3B2))),
              ],
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
                title: const Text('Build house'),
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
          if (c.rentBoost > 0 && !ts.mortgaged && ts.boosts < 3) {
            items.add(ListTile(
                title: Text('Boost rent (\$${price * c.rentBoost ~/ 100})'),
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
            c.expropriation > 0 &&
            ts.houses == 0 &&
            !ts.mortgaged) {
          items.add(ListTile(
              title: Text('Seize (\$${price * c.expropriation ~/ 100})'),
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
          if (s.gameEndsAt != null && s.view?.finished != true) ...[
            _Countdown(endsAt: s.gameEndsAt!),
            const SizedBox(width: 8),
          ],
          const _MuteButton(),
        ]),
        const SizedBox(height: 4),
        Text(_status(), style: const TextStyle(fontWeight: FontWeight.w600)),
        const SizedBox(height: 6),
        _Actions(s: s),
        const SizedBox(height: 6),
        Expanded(child: _EventLog(log: s.log)),
      ]),
    );
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

/// Ticking countdown to the timed-game deadline (ADR-0010). Turns amber
/// under a minute so players feel the pressure.
class _Countdown extends StatefulWidget {
  final DateTime endsAt;
  const _Countdown({required this.endsAt});

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final left = widget.endsAt.difference(DateTime.now());
    final secs = left.isNegative ? 0 : left.inSeconds;
    final mmss =
        '${(secs ~/ 60).toString().padLeft(2, '0')}:${(secs % 60).toString().padLeft(2, '0')}';
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Icon(Icons.timer,
          size: 18,
          color: secs <= 60 ? const Color(0xFFC0564F) : const Color(0xFF2A2A2A)),
      const SizedBox(width: 4),
      Text(mmss,
          style: TextStyle(
            fontWeight: FontWeight.bold,
            fontFeatures: const [FontFeature.tabularFigures()],
            color:
                secs <= 60 ? const Color(0xFFC0564F) : const Color(0xFF2A2A2A),
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
    return IconButton(
      iconSize: 18,
      padding: EdgeInsets.zero,
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(),
      tooltip: sfx.enabled ? 'Mute sound' : 'Unmute sound',
      icon: Icon(sfx.enabled ? Icons.volume_up : Icons.volume_off,
          color: const Color(0xFF2A2A2A)),
      onPressed: () => setState(() => sfx.enabled = !sfx.enabled),
    );
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

    Widget btn(String label, Map<String, dynamic> cmd, {bool primary = true}) {
      return primary
          ? FilledButton(onPressed: () => s.sendCmd(cmd), child: Text(label))
          : OutlinedButton(onPressed: () => s.sendCmd(cmd), child: Text(label));
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
        FilledButton(
          onPressed: () => s
              .sendCmd({'type': 'bid', 'amount': int.tryParse(_bid.text) ?? 0}),
          child: const Text('Bid'),
        ),
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
                    Expanded(
                      child: FilledButton(
                        onPressed: s.sendPlayAgain,
                        child: const Text('Play again'),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: OutlinedButton(
                        onPressed: s.leave,
                        child: const Text('Continue'),
                      ),
                    ),
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
            Text('ROOM ${s.code}',
                style: const TextStyle(
                    fontSize: 12, color: Color(0xFF9AA3B2), letterSpacing: 1)),
            const SizedBox(height: 6),
            _players(),
            if (s.view == null) ...[
              const SizedBox(height: 8),
              FilledButton(
                onPressed:
                    s.seat == 0 && s.seats.length >= 2 ? s.sendStart : null,
                child: const Text('Start game'),
              ),
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
          child: OutlinedButton(
            style: OutlinedButton.styleFrom(
                foregroundColor: const Color(0xFFC0564F)),
            onPressed: () async {
              final ok = await showDialog<bool>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: const Text('Resign from the game?'),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(ctx, false),
                        child: const Text('Cancel')),
                    TextButton(
                        onPressed: () => Navigator.pop(ctx, true),
                        child: const Text('Resign')),
                  ],
                ),
              );
              if (ok == true) s.sendCmd({'type': 'resign'});
            },
            child: const Text('Resign'),
          ),
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
        if (seatInfo?.connected == false) '(offline)',
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
                TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'accept_trade', 'trade': o.id}),
                    child: const Text('Accept')),
                TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'decline_trade', 'trade': o.id}),
                    child: const Text('Refuse')),
              ],
              if (o.from == s.seat)
                TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'cancel_trade', 'trade': o.id}),
                    child: const Text('Cancel')),
            ]),
          ]),
        ),
      if (v != null && !v.finished)
        OutlinedButton(
          onPressed: () => showDialog<void>(
              context: context, builder: (ctx) => TradeDialog(s: s)),
          child: const Text('New offer'),
        ),
    ]);
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
            IconButton(
              icon: const Icon(Icons.close, size: 16),
              onPressed: s.dismissFeedback,
              tooltip: 'Dismiss',
            ),
          ]),
          Row(children: [
            for (var star = 1; star <= 5; star++)
              IconButton(
                icon: Icon(
                  star <= _rating ? Icons.star : Icons.star_border,
                  color: const Color(0xFFD8B45A),
                ),
                onPressed: () => setState(() => _rating = star),
              ),
          ]),
          TextField(
            controller: _comment,
            maxLength: 500,
            decoration: const InputDecoration(
                labelText: 'Anything to add? (optional)', counterText: ''),
          ),
          const SizedBox(height: 6),
          FilledButton(
            onPressed: _rating == 0
                ? null
                : () => s.sendFeedback(_rating, _comment.text),
            child: const Text('Send'),
          ),
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
        TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close')),
        FilledButton(
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
        ),
      ],
    );
  }
}
