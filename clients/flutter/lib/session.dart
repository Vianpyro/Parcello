/// Connection + game state in one ChangeNotifier: the Dart equivalent of the
/// reference web client's `st` object and message switch. The server is
/// authoritative; this only projects what it pushes.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'board.dart' show CashFloater;
import 'protocol.dart';
import 'session_storage.dart';
import 'sfx.dart';

class GameSession extends ChangeNotifier {
  WebSocketChannel? _ws;
  StreamSubscription? _sub;

  /// Reconnect tokens by room code (ADR-0008), persisted so a restarted
  /// client can still prove seat ownership (file on native, localStorage on
  /// web - see session_storage.dart). Best-effort: storage errors only cost
  /// the persistence, never the session.
  final Map<String, String> _reconnectTokens = {};

  GameSession() {
    _reconnectTokens.addAll(loadReconnectTokens());
  }

  void _saveToken(String code, String token) {
    _reconnectTokens[code] = token;
    saveReconnectTokens(_reconnectTokens);
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

  /// Floor on the decision window for a jailed seat still choosing its
  /// exit (Legal Route / Corruption / jail card) - mirrors the server's
  /// `JAIL_DECISION_SECS` (`crates/server/src/room.rs`): an ordinary blitz
  /// turn is too short for that decision (2026-07 playtest feedback).
  static const _jailDecisionSecs = 20;

  /// `turnSeconds`, floored to `_jailDecisionSecs` while whoever is
  /// currently acting is jailed and hasn't chosen an exit yet - a local
  /// approximation of the server's own floor (`turn_limit_secs`), so the
  /// displayed countdown doesn't appear to expire well before the server
  /// would actually auto-play. The server alone decides when it fires.
  int? _effectiveTurnSeconds() {
    final base = turnSeconds;
    if (base == null) return null;
    final v = view;
    final acting = v?.players.elementAtOrNull(v.current);
    final jailedDeciding = acting?.inJail == true && acting?.jailRoute == null;
    return jailedDeciding && base < _jailDecisionSecs ? _jailDecisionSecs : base;
  }

  /// Personal time bank in seconds (ADR-0023); null when off. `banks` is the
  /// live per-seat remaining amount from the latest Update; `bankEndsAt` is
  /// derived from it and only starts counting down once `turnEndsAt` passes
  /// - the bank is a flat reserve until the plain turn window is spent.
  int? timeBankSeconds;
  List<int>? banks;
  DateTime? bankEndsAt;

  /// Sealed-bid / bribe-vote window deadlines (ADR-0018/ADR-0024): a local
  /// approximation of the server's window (12s for bids, 5s for votes),
  /// set the moment we first see the phase and cleared once it's gone -
  /// the server alone resolves the window, and its own clock only starts
  /// once the table's render acks land (ADR-0028), so this is a rough
  /// upper bound, not a precise mirror.
  DateTime? bidEndsAt;
  DateTime? voteEndsAt;
  void _trackTimedWindows() {
    bidEndsAt = view?.turn.type == 'blind_auction'
        ? (bidEndsAt ?? DateTime.now().add(const Duration(seconds: 12)))
        : null;
    voteEndsAt = view?.turn.type == 'bribe_vote'
        ? (voteEndsAt ?? DateTime.now().add(const Duration(seconds: 5)))
        : null;
  }

  /// Latest movement card played, for the center-of-board flash (ADR-0017).
  /// `cardSeq` bumps on every play so the overlay re-triggers even on a
  /// repeated value.
  int cardSeq = 0;
  int cardValue = 0;

  /// Latest drawn chance/community card text, revealed over the board for a
  /// beat (ADR-0028) - without it, card effects were invisible and
  /// card-driven relocations looked like unexplained teleports.
  int chanceCardSeq = 0;
  String chanceCardText = '';

  /// Latest spotlighted tile name, for a brief banner (ADR-0026/0028).
  int spotlightFlashSeq = 0;
  String spotlightFlashText = '';

  /// Director-driven pawn positions (ADR-0028): the board renders these,
  /// advanced beat by beat through each Update's events so multi-hop
  /// chains (chance -> reveal -> teleport) read as separate moments; they
  /// converge on the authoritative view positions once the Update applies.
  final Map<int, int> displayPositions = {};

  /// Transient cash-delta floaters over board tiles (ADR-0028 extras).
  final List<CashFloater> floaters = [];
  int _floaterId = 0;

  /// Tile the hovered movement card would land on (null = no hover): the
  /// board outlines it so players see where a card takes them before
  /// committing (2026-07 playtest feedback).
  int? hoverTile;
  void setHoverTile(int? tile) {
    if (hoverTile == tile) return;
    hoverTile = tile;
    notifyListeners();
  }

  /// Updates not yet rendered (ADR-0028): played strictly in order, one at
  /// a time; each is acked to the server once its beats finish.
  final List<Map<String, dynamic>> _pendingUpdates = [];
  bool _animating = false;

  /// Bumped whenever the room context resets (leave, disconnect, new game)
  /// so a mid-flight beat sequence aborts instead of applying stale state.
  int _updateEpoch = 0;
  bool _disposed = false;

  /// True while an Update's beats are still playing (ADR-0028). UI
  /// countdowns (turn clock, time bank, bid/vote windows) should freeze
  /// rather than tick down during this - none of them are actually
  /// consumed server-side while the table is still rendering.
  bool get isAnimating => _animating;

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
    _resetDirector();
    code = null;
    gameEndsAt = null;
    turnEndsAt = null;
    bidEndsAt = null;
    voteEndsAt = null;
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
    _resetDirector();
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
        // Set before turnEndsAt below - a reconnect mid-game may land
        // straight into a jailed seat's extended decision window, and
        // _effectiveTurnSeconds needs the fresh view to know that.
        if (msg['view'] != null) {
          view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        }
        gameEndsAt = _deadlineFrom(msg['time_remaining']);
        turnSeconds = msg['turn_seconds'] as int?;
        turnEndsAt = _deadlineFrom(_effectiveTurnSeconds());
        timeBankSeconds = msg['time_bank_seconds'] as int?;
        banks = null;
        _resetDirector();
        _trackTimedWindows();
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
        _resetDirector();
        feedbackDone = false;
        gameEndsAt = _deadlineFrom(msg['time_remaining']);
        turnSeconds = msg['turn_seconds'] as int?;
        turnEndsAt = _deadlineFrom(_effectiveTurnSeconds());
        timeBankSeconds = msg['time_bank_seconds'] as int?;
        banks = null;
        _trackTimedWindows();
        sfx.gameStart();
        _log('Game started.');
      case 'update':
        // Queued rather than applied (ADR-0028): the animation director
        // below plays each Update as paced beats, applies the
        // authoritative view at the end, then acks so the server's gated
        // timers (bid window, turn clock, bot pacing) can start.
        _pendingUpdates.add(msg);
        _drainUpdates();
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

  // -- Animation director (ADR-0028) -----------------------------------

  void _drainUpdates() {
    if (_disposed || _animating || _pendingUpdates.isEmpty) return;
    _animating = true;
    final msg = _pendingUpdates.removeAt(0);
    _playUpdate(msg).whenComplete(() {
      _animating = false;
      // Without this, a countdown frozen via isAnimating could keep
      // showing its last frozen value until some unrelated rebuild
      // happened to come along and notice the flag had flipped.
      if (!_disposed) notifyListeners();
      _drainUpdates();
    });
  }

  /// Plays one Update as a sequence of paced beats - pawn slides, card
  /// reveals, cash floaters - so a multi-event burst (move -> chance card
  /// -> teleport -> auction) reads as separate moments instead of one
  /// instant snap. The authoritative view applies only once the beats
  /// finish, which is also what holds the sealed-bid overlay and the local
  /// turn countdown back until this client has visually arrived; the final
  /// ack then releases the server's own gated timers.
  Future<void> _playUpdate(Map<String, dynamic> msg) async {
    final epoch = _updateEpoch;
    final events = (msg['events'] as List).cast<Map<String, dynamic>>();
    final animate = view != null && joined;
    for (final e in events) {
      _log(describeEvent(
          e, playerName, tileName, content?.marketEventName ?? (id) => id));
      if (animate) {
        await _playBeat(e);
        if (_disposed || epoch != _updateEpoch) return; // context changed
      } else if (e['type'] == 'movement_card_played') {
        // Fresh join, no prior view: show the flash, skip the pacing.
        cardValue = e['value'] as int;
        cardSeq++;
      }
    }
    // Victory-point deltas float over each player's tile once the beats
    // are done and the authoritative totals land (ADR-0020 visibility:
    // covers every source at once - groups completed/lost, conglomerates,
    // utilities, the round bonus - without re-deriving the scoring).
    final oldVp = [
      for (final p in view?.players ?? const <PlayerView>[]) p.victoryPoints
    ];
    view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
    _syncDisplayPositions();
    if (oldVp.isNotEmpty) {
      for (var i = 0; i < view!.players.length && i < oldVp.length; i++) {
        _floatVp(i, view!.players[i].victoryPoints - oldVp[i]);
      }
    }
    turnEndsAt = _deadlineFrom(_effectiveTurnSeconds());
    banks = (msg['banks'] as List?)?.cast<int>();
    _trackTimedWindows();
    // The bank only starts draining once the plain turn window is
    // spent; until then it shows the flat reserve (ADR-0023).
    bankEndsAt = (turnEndsAt != null && banks != null && seat != null)
        ? turnEndsAt!.add(Duration(seconds: banks![seat!]))
        : null;
    final seq = msg['seq'] as int? ?? 0;
    if (seq > 0) {
      _ws?.sink.add(jsonEncode({'type': 'animation_done', 'through_seq': seq}));
    }
    notifyListeners();
  }

  /// One event's visual beat: mutate the display state, notify, then wait
  /// for however long that visual runs. Events with no visual fall through
  /// instantly.
  Future<void> _playBeat(Map<String, dynamic> e) async {
    switch (e['type']) {
      case 'movement_card_played':
        cardValue = e['value'] as int;
        cardSeq++;
        sfx.diceRoll();
        notifyListeners();
      // The pawn slide itself follows in this Update's `moved` beat.
      case 'moved':
        final p = e['player'] as int;
        final from = e['from'] as int? ?? displayPositions[p] ?? 0;
        final to = e['to'] as int;
        displayPositions[p] = to;
        notifyListeners();
        await Future.delayed(_moveDuration(from, to));
      case 'went_to_jail':
        final p = e['player'] as int;
        final jail = content?.board.indexWhere((t) => t.kind == 'jail') ?? -1;
        if (jail >= 0) displayPositions[p] = jail;
        notifyListeners();
        await Future.delayed(const Duration(milliseconds: 1100));
      case 'card_drawn':
        chanceCardText = e['text'] as String? ?? '';
        chanceCardSeq++;
        notifyListeners();
        await Future.delayed(const Duration(milliseconds: 1700));
      case 'spotlight_started':
        // Naming a tile alone didn't explain what happened (2026-07
        // playtest feedback) - spell out the actual effect.
        final pct = e['rent_pct'] as int;
        final turns = e['duration_turns'] as int;
        final span = turns <= 0
            ? 'until the next Exposition landing'
            : 'for $turns turns';
        spotlightFlashText =
            '${tileName(e['tile'] as int)} is in the spotlight!\n'
            '+$pct% rent $span';
        spotlightFlashSeq++;
        notifyListeners();
        await Future.delayed(const Duration(milliseconds: 1800));
      case 'salary_paid':
        _floatCash(e['player'] as int, e['amount'] as int);
        await Future.delayed(const Duration(milliseconds: 500));
      case 'rent_paid':
        _floatCash(e['from'] as int, -(e['amount'] as int));
        await Future.delayed(const Duration(milliseconds: 500));
      case 'tax_paid':
        _floatCash(e['player'] as int, -(e['amount'] as int));
        await Future.delayed(const Duration(milliseconds: 500));
      case 'cash_adjusted':
        _floatCash(e['player'] as int, e['delta'] as int);
        await Future.delayed(const Duration(milliseconds: 500));
    }
  }

  /// Mirrors `_PawnLayer`'s hop timing (260ms per tile, plus its 260ms
  /// wind-up) so a beat waits for the glide it just triggered.
  Duration _moveDuration(int from, int to) {
    final n = content?.board.length ?? 0;
    if (n == 0) return const Duration(milliseconds: 700);
    final forward = (to - from) % n;
    final glide = (forward >= 1 && forward <= 12)
        ? (forward * 260).clamp(400, 3200)
        : 700;
    return Duration(milliseconds: 260 + glide + 150);
  }

  /// Spawns a rising "+$X"/"-$X" over the player's current tile (ADR-0028
  /// extras); removes it once its own animation has run out.
  void _floatCash(int player, int amount) {
    if (amount == 0) return;
    final sign = amount > 0 ? '+' : '-';
    _float(player, '$sign\$${amount.abs()}', gain: amount > 0);
  }

  /// Spawns a rising "+N VP"/"-N VP" over the player's tile: the VP race is
  /// the primary win condition (ADR-0020) but its gains were invisible -
  /// nobody understood when or why points moved (2026-07 playtest).
  void _floatVp(int player, int delta) {
    if (delta == 0) return;
    final sign = delta > 0 ? '+' : '-';
    _float(player, '$sign${delta.abs()} VP', gain: delta > 0, vp: true);
  }

  void _float(int player, String text, {required bool gain, bool vp = false}) {
    final tile = displayPositions[player] ??
        view?.players.elementAtOrNull(player)?.position ??
        0;
    final f = CashFloater(
        id: _floaterId++, tile: tile, text: text, gain: gain, vp: vp);
    floaters.add(f);
    notifyListeners();
    Timer(const Duration(milliseconds: 1200), () {
      if (_disposed) return;
      floaters.remove(f);
      notifyListeners();
    });
  }

  void _syncDisplayPositions() {
    final v = view;
    if (v == null) return;
    for (var i = 0; i < v.players.length; i++) {
      displayPositions[i] = v.players[i].position;
    }
  }

  /// Aborts any in-flight beat sequence and clears the director's
  /// transient state - called whenever the room context changes.
  void _resetDirector() {
    _updateEpoch++;
    _pendingUpdates.clear();
    _animating = false;
    floaters.clear();
    displayPositions.clear();
    _syncDisplayPositions();
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
    _disposed = true;
    disconnect();
    super.dispose();
  }
}
