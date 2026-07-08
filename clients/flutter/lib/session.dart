/// Connection + game state in one ChangeNotifier: the Dart equivalent of the
/// reference web client's `st` object and message switch. The server is
/// authoritative; this only projects what it pushes.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'protocol.dart';
import 'sfx.dart';

class GameSession extends ChangeNotifier {
  WebSocketChannel? _ws;
  StreamSubscription? _sub;

  /// Reconnect tokens by room code (ADR-0008), persisted so a restarted
  /// client can still prove seat ownership. Best-effort: IO errors only
  /// cost the persistence, never the session.
  final Map<String, String> _reconnectTokens = {};
  late final File _tokenFile = () {
    final base =
        Platform.environment['APPDATA'] ?? Platform.environment['HOME'] ?? '.';
    return File('$base/parcello/reconnect.json');
  }();

  GameSession() {
    try {
      final saved = jsonDecode(_tokenFile.readAsStringSync()) as Map;
      _reconnectTokens.addAll(saved.cast<String, String>());
    } catch (_) {}
  }

  void _saveToken(String code, String token) {
    _reconnectTokens[code] = token;
    try {
      _tokenFile.parent.createSync(recursive: true);
      _tokenFile.writeAsStringSync(jsonEncode(_reconnectTokens));
    } catch (_) {}
  }

  // ponytail: the issuer URL rides in the reconnect-token file under a
  // reserved key (room codes are exactly 5 uppercase letters, no clash);
  // split into a prefs file if more settings ever appear.
  String get savedIssuer => _reconnectTokens['_issuer'] ?? '';
  void saveIssuer(String url) => _saveToken('_issuer', url);

  int? seat;
  String? code;
  GameContent? content;
  ClientView? view;
  List<SeatInfo> seats = [];
  /// Current room settings (timers + rules, ADR-0015); the host edits them
  /// in the lobby. Null before the first Joined/Lobby message.
  RoomSettings? settings;
  final List<String> log = [];
  String loginMessage = '';
  bool joined = false;

  /// True once the socket is up (the menu is shown). Identity is remembered
  /// here so create/join over the same connection need no re-entry.
  bool connected = false;
  String _authName = '';
  String _authToken = '';

  /// Public accessors for the stored auth identity (guest name or token).
  String get authName => _authName;
  String get authToken => _authToken;

  /// Post-game survey shown once per game; answering or dismissing hides it.
  bool feedbackDone = false;

  /// When set, the game is time-boxed and ends at this wall-clock instant
  /// (ADR-0010); the UI shows a local countdown. Null for untimed games.
  DateTime? gameEndsAt;

  /// Per-turn time limit in seconds when the server runs with
  /// `--turn-timeout`; null when the AFK timer is off. `turnEndsAt` is the
  /// current turn's deadline, reset on every Update (server resets its AFK
  /// clock on each accepted command).
  int? turnSeconds;
  DateTime? turnEndsAt;

  /// Personal time bank in seconds (ADR-0023); null when off. `banks` is the
  /// live per-seat remaining amount from the latest Update; `bankEndsAt` is
  /// derived from it and only starts counting down once `turnEndsAt` passes
  /// - the bank is a flat reserve until the plain turn window is spent.
  int? timeBankSeconds;
  List<int>? banks;
  DateTime? bankEndsAt;

  /// Sealed-bid window deadline (ADR-0018): a local approximation of the
  /// server's 5s window, set the moment we first see the phase and cleared
  /// once it's gone - the server alone resolves the window.
  DateTime? bidEndsAt;
  void _trackBidWindow() {
    if (view?.turn.type == 'blind_auction') {
      bidEndsAt ??= DateTime.now().add(const Duration(seconds: 5));
    } else {
      bidEndsAt = null;
    }
  }

  /// Latest dice roll for the center-of-board display. `diceSeq` bumps on
  /// every roll so the overlay re-triggers even on a repeated value.
  int diceSeq = 0;
  int diceD1 = 0;
  int diceD2 = 0;

  /// Net worth of a seat, mirroring `GameState::net_worth` on the server so
  /// the shown ranking predicts the timed-game winner: cash + property
  /// equity (price, or price/2 mortgaged) + houses at build cost.
  int netWorth(int seat) {
    final v = view, c = content;
    if (v == null || c == null || seat >= v.players.length) return 0;
    var worth = v.players[seat].cash;
    for (var i = 0; i < c.board.length && i < v.tiles.length; i++) {
      if (v.tiles[i].owner != seat) continue;
      final def = c.board[i];
      if (!def.isProperty) continue;
      final price = def.price ?? 0;
      worth += v.tiles[i].mortgaged ? price ~/ 2 : price;
      worth += v.tiles[i].houses * def.houseCost;
    }
    return worth;
  }

  bool get myTurn => view != null && seat != null && view!.current == seat;

  String playerName(int i) =>
      view?.players.elementAtOrNull(i)?.name ??
      seats.elementAtOrNull(i)?.name ??
      'seat $i';

  String tileName(int i) => content?.board.elementAtOrNull(i)?.name ?? 'tile $i';

  /// Opens the socket to `url` and remembers the identity (guest `name`, or
  /// an OIDC `token`, ADR-0009). Does NOT enter a room - create/join happen
  /// later from the menu over this same connection.
  void connect(String url, String name, {String token = ''}) {
    disconnect();
    _authName = name;
    _authToken = token;
    loginMessage = 'Connecting...';
    notifyListeners();
    final WebSocketChannel ws;
    try {
      ws = WebSocketChannel.connect(Uri.parse(url));
    } catch (e) {
      loginMessage = 'Bad server URL: $e';
      notifyListeners();
      return;
    }
    _ws = ws;
    _sub = ws.stream.listen(
      (data) => _handle(jsonDecode(data as String) as Map<String, dynamic>),
      onDone: _onClosed,
      onError: (Object e) {
        loginMessage = 'Connection failed: $e';
        _onClosed();
      },
    );
    // Only reveal the menu once the socket is actually up.
    ws.ready.then((_) {
      connected = true;
      loginMessage = '';
      notifyListeners();
    }).catchError((Object e) {
      loginMessage = 'Cannot reach server: $e';
      _onClosed();
    });
  }

  Map<String, dynamic> _auth(String code) => {
        if (_authToken.isNotEmpty)
          'token': _authToken
        else
          'guest_name': _authName,
        if (_reconnectTokens[code] != null) 'reconnect': _reconnectTokens[code],
      };

  /// Host a new private room. `mods` picks its mod set (ADR-0006).
  void createGame({List<String> mods = const []}) {
    _ws?.sink.add(jsonEncode({
      'type': 'create',
      'auth': _auth(''),
      if (mods.isNotEmpty) 'mods': mods,
    }));
  }

  /// Join a private room by its 5-letter code.
  void joinGame(String roomCode) {
    final c = roomCode.trim().toUpperCase();
    _ws?.sink.add(jsonEncode({'type': 'join', 'code': c, 'auth': _auth(c)}));
  }

  void disconnect() {
    // Cancel first so a deliberate close does not fire `_onClosed`.
    _sub?.cancel();
    _sub = null;
    _ws?.sink.close();
    _ws = null;
  }

  /// Replay in the same room (server picks whoever is still connected).
  void sendPlayAgain() {
    _ws?.sink.add(jsonEncode({'type': 'play_again'}));
  }

  /// Leave the room but stay connected, returning to the menu.
  void leaveRoom() {
    _ws?.sink.add(jsonEncode({'type': 'leave'}));
    joined = false;
    view = null;
    code = null;
    gameEndsAt = null;
    turnEndsAt = null;
    bidEndsAt = null;
    loginMessage = '';
    notifyListeners();
  }

  /// Close the connection entirely, returning to the connect screen.
  void disconnectFromServer() {
    disconnect();
    connected = false;
    joined = false;
    view = null;
    code = null;
    loginMessage = '';
    notifyListeners();
  }

  void _onClosed() {
    _sub?.cancel();
    _sub = null;
    _ws = null;
    connected = false;
    joined = false;
    view = null;
    if (loginMessage.isEmpty || loginMessage == 'Connecting...') {
      loginMessage = 'Disconnected from server.';
    }
    notifyListeners();
  }

  void sendCmd(Map<String, dynamic> cmd) {
    _ws?.sink.add(jsonEncode({'type': 'cmd', 'cmd': cmd}));
  }

  void sendStart() {
    _ws?.sink.add(jsonEncode({'type': 'start'}));
  }

  void addBot() {
    _ws?.sink.add(jsonEncode({'type': 'add_bot'}));
  }

  void removeBot() {
    _ws?.sink.add(jsonEncode({'type': 'remove_bot'}));
  }

  /// Host only: replace the room settings (ADR-0015). `settings` is the raw
  /// wire map; the server clamps and broadcasts the applied values back.
  void configure(Map<String, dynamic> settings) {
    _ws?.sink.add(jsonEncode({'type': 'configure', 'settings': settings}));
  }

  /// Post-game survey answer; `rating` 1-5, empty comment omitted.
  void sendFeedback(int rating, String comment) {
    _ws?.sink.add(jsonEncode({
      'type': 'feedback',
      'rating': rating,
      if (comment.trim().isNotEmpty) 'comment': comment.trim(),
    }));
    feedbackDone = true;
    notifyListeners();
  }

  void dismissFeedback() {
    feedbackDone = true;
    notifyListeners();
  }

  void _handle(Map<String, dynamic> msg) {
    switch (msg['type']) {
      case 'room_created':
        code = msg['code'] as String;
      case 'joined':
        code = msg['code'] as String;
        seat = msg['seat'] as int;
        content = GameContent.fromJson(msg['content'] as Map<String, dynamic>);
        seats = _seatList(msg['players']);
        settings = RoomSettings.fromJson(msg['settings'] as Map<String, dynamic>);
        if (msg['reconnect'] != null) {
          _saveToken(code!, msg['reconnect'] as String);
        }
        gameEndsAt = _deadlineFrom(msg['time_remaining']);
        turnSeconds = msg['turn_seconds'] as int?;
        turnEndsAt = _deadlineFrom(turnSeconds);
        timeBankSeconds = msg['time_bank_seconds'] as int?;
        banks = null;
        if (msg['view'] != null) {
          view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        }
        _trackBidWindow();
        joined = true;
        loginMessage = '';
        _log('Joined room $code. Mods: ${content!.modIds.join(', ')}');
      case 'lobby':
        final incoming = _seatList(msg['players']);
        _announceSeatChanges(incoming);
        seats = incoming;
        settings = RoomSettings.fromJson(msg['settings'] as Map<String, dynamic>);
      case 'game_started':
        view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        feedbackDone = false;
        gameEndsAt = _deadlineFrom(msg['time_remaining']);
        turnSeconds = msg['turn_seconds'] as int?;
        turnEndsAt = _deadlineFrom(turnSeconds);
        timeBankSeconds = msg['time_bank_seconds'] as int?;
        banks = null;
        _trackBidWindow();
        sfx.gameStart();
        _log('Game started.');
      case 'update':
        view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        turnEndsAt = _deadlineFrom(turnSeconds);
        banks = (msg['banks'] as List?)?.cast<int>();
        _trackBidWindow();
        // The bank only starts draining once the plain turn window is
        // spent; until then it shows the flat reserve (ADR-0023).
        bankEndsAt = (turnEndsAt != null && banks != null && seat != null)
            ? turnEndsAt!.add(Duration(seconds: banks![seat!]))
            : null;
        for (final e in msg['events'] as List) {
          final ev = e as Map<String, dynamic>;
          if (ev['type'] == 'dice_rolled') {
            diceD1 = ev['d1'] as int;
            diceD2 = ev['d2'] as int;
            diceSeq++;
            sfx.diceRoll();
          }
          _log(describeEvent(
              ev, playerName, tileName, content?.marketEventName ?? (id) => id));
        }
      case 'rejected':
        sfx.error();
        _log('Rejected: ${msg['error']['code']}');
      case 'error':
        sfx.error();
        if (!joined) loginMessage = msg['message'] as String;
        _log('Error: ${msg['message']}');
    }
    notifyListeners();
  }

  DateTime? _deadlineFrom(dynamic secs) =>
      secs == null ? null : DateTime.now().add(Duration(seconds: secs as int));

  List<SeatInfo> _seatList(dynamic players) => (players as List)
      .map((s) => SeatInfo.fromJson(s as Map<String, dynamic>))
      .toList();

  /// Plays a join/leave cue when the lobby seat list changes (new player,
  /// bot added/removed). Compares against the previous `seats`, so this must
  /// run before `seats` is overwritten.
  void _announceSeatChanges(List<SeatInfo> incoming) {
    final before = seats.map((p) => p.playerId).toSet();
    final after = incoming.map((p) => p.playerId).toSet();
    if (after.difference(before).isNotEmpty) sfx.playerJoin();
    if (before.difference(after).isNotEmpty) sfx.playerLeave();
  }

  void _log(String line) {
    log.add(line);
    if (log.length > 500) log.removeAt(0);
  }

  @override
  void dispose() {
    disconnect();
    super.dispose();
  }
}
