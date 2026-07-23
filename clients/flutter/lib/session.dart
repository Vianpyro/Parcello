/// Connection + game state in one ChangeNotifier: the Dart equivalent of the
/// reference web client's `st` object and message switch. The server is
/// authoritative; this only projects what it pushes.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'auth_manager.dart';
import 'director.dart';
import 'l10n/app_localizations.dart';
import 'l10n/app_localizations_en.dart';
import 'motion.dart';
import 'protocol.dart';
import 'session_storage.dart';
import 'sfx.dart';
import 'stage.dart';

class GameSession extends ChangeNotifier {
  /// What the board is currently *showing*, as opposed to what the server says
  /// is *true* (`view`). A separate notifier: animation frames repaint the
  /// board, they must not repaint the action panel's text fields.
  final StageState stage = StageState();

  WebSocketChannel? _ws;
  StreamSubscription? _sub;

  /// Reconnect tokens by room code (ADR-0008), persisted so a restarted
  /// client can still prove seat ownership (file on native, localStorage on
  /// web - see session_storage.dart). Best-effort: storage errors only cost
  /// the persistence, never the session.
  final Map<String, String> _reconnectTokens = {};

  GameSession() {
    _reconnectTokens.addAll(loadReconnectTokens());
    localeTag.value = _reconnectTokens['_locale'] ?? '';
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

  /// Chosen UI language tag, '' = follow the system. Persisted beside the
  /// issuer under another reserved key. It carries its own notifier instead of
  /// riding `notifyListeners()`: the locale sits on `MaterialApp`, and this
  /// session notifies on every server update - rebuilding the whole app tree
  /// that often would undo the care taken to keep the board's centre panel
  /// alive across updates.
  final ValueNotifier<String> localeTag = ValueNotifier('');

  void setLocaleTag(String tag) {
    localeTag.value = tag;
    _saveToken('_locale', tag);
  }

  int? seat;

  /// Watching without a seat (ADR-0035): the game screen renders the
  /// spectator view and offers no actions.
  bool spectating = false;
  String? code;
  GameContent? content;
  ClientView? view;
  List<SeatInfo> seats = [];

  /// Current room settings (timers + rules, ADR-0015); the host edits them
  /// in the lobby. Null before the first Joined/Lobby message.
  RoomSettings? settings;

  /// Localizations for the event log, set by the widget tree (which has a
  /// BuildContext) on every frame - see ParcelloApp. Non-null from the first
  /// rendered frame, i.e. before any server message is ever processed.
  AppLocalizations? l10n;
  final List<String> log = [];
  String loginMessage = '';
  bool joined = false;

  /// True once the socket is up (the menu is shown). Identity is remembered
  /// here so create/join over the same connection need no re-entry.
  bool connected = false;

  /// The credential this session plays under, and its whole lifecycle
  /// (ADR-0037): a guest name, or an OIDC grant that renews itself before
  /// `exp` so a long game never outlives its token.
  AuthManager auth = AuthManager.guest('');

  String get authName => auth.displayName;

  /// True while the socket is down and being re-established (ADR-0037).
  /// The UI stays on the game screen and says so rather than dropping the
  /// player back to sign-in - the seat is still held server-side and the
  /// rejoin below reclaims it.
  bool reconnecting = false;

  /// True once the credential is past renewal (refresh refused, or none
  /// was ever granted and the token has expired). Only then does the
  /// player need to sign in again.
  bool get signInRequired => auth.signInRequired;

  /// Post-game survey shown once per game; answering or dismissing hides it.
  bool feedbackDone = false;

  // -- First-game coach marks ------------------------------------------

  /// Hint ids already dismissed, persisted beside the reconnect tokens
  /// under a reserved key (room codes are 5 uppercase letters, no clash).
  /// One contextual hint shows at a time, the first time its situation
  /// comes up; "replay the tips" in the menu clears the set.
  late final Set<String> _seenHints = {
    ...?_reconnectTokens['_hints']?.split(',').where((h) => h.isNotEmpty),
  };

  bool hintSeen(String id) => _seenHints.contains(id);

  void dismissHint(String id) {
    if (!_seenHints.add(id)) return;
    _saveToken('_hints', _seenHints.join(','));
    notifyListeners();
  }

  void resetHints() {
    _seenHints.clear();
    _saveToken('_hints', '');
    notifyListeners();
  }

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
    return jailedDeciding && base < _jailDecisionSecs
        ? _jailDecisionSecs
        : base;
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

  /// Whether resigning is currently a meaningful action: an active game
  /// exists, it is not over, I hold a seat (a spectator has none, ADR-0035),
  /// and I am not already bankrupt (nothing left to forfeit).
  bool get canResign =>
      view != null &&
      !view!.finished &&
      seat != null &&
      !view!.players[seat!].bankrupt;

  String playerName(int i) =>
      view?.players.elementAtOrNull(i)?.name ??
      seats.elementAtOrNull(i)?.name ??
      'seat $i';

  String tileName(int i) =>
      content?.board.elementAtOrNull(i)?.name ?? 'tile $i';

  // -- Connection lifecycle (ADR-0036 socket, ADR-0037 recovery) --------

  /// Server this session belongs to, so a dropped socket can be
  /// re-established without asking the player for anything.
  String _serverUrl = '';

  /// Set by every deliberate close (leave the server, dispose) so
  /// `_onClosed` knows not to fight it. Only a close nobody asked for is
  /// retried.
  bool _closedOnPurpose = false;

  Timer? _reconnectTimer;
  int _reconnectAttempts = 0;

  /// Room to re-enter once the socket is back, and whether we were
  /// watching rather than seated. Null when the player is in the menu -
  /// then reconnecting just restores the socket.
  String? _rejoinCode;
  bool _rejoinSpectating = false;

  /// True between sending an automatic rejoin and its answer, so a
  /// `Rejected`/`Error` reply can be recognised as "the room is gone" and
  /// return the player to the menu instead of leaving a game screen that
  /// will never update.
  bool _rejoinPending = false;

  /// Backoff schedule: 0.5s doubling to a 15s ceiling, given up after
  /// `_maxReconnectAttempts`. Long enough to ride out a proxy restart or a
  /// Wi-Fi roam, short enough that the first retry is nearly instant.
  static const _maxReconnectAttempts = 8;
  static const _reconnectCeiling = Duration(seconds: 15);

  Duration _backoff(int attempt) {
    final ms = 500 * (1 << attempt);
    return ms >= _reconnectCeiling.inMilliseconds
        ? _reconnectCeiling
        : Duration(milliseconds: ms);
  }

  /// Opens the socket to `url` under `identity` (a guest name, or an OIDC
  /// grant that renews itself - ADR-0009/ADR-0037). Does NOT enter a room:
  /// create/join happen later from the menu over this same connection.
  void connect(String url, AuthManager identity) {
    disconnect();
    auth = identity;
    auth.onChanged = notifyListeners;
    _serverUrl = url;
    _reconnectAttempts = 0;
    _rejoinCode = null;
    loginMessage = 'Connecting...';
    notifyListeners();
    _open(url);
  }

  /// Opens one socket. Shared by the first connect and every reconnect
  /// attempt, so both paths wire the stream identically.
  void _open(String url) {
    _closedOnPurpose = false;
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
    ws.ready
        .then((_) {
          connected = true;
          reconnecting = false;
          _reconnectAttempts = 0;
          loginMessage = '';
          notifyListeners();
          // A socket that came back mid-game reclaims the seat by itself:
          // the server rejoins by identity and pushes the full snapshot
          // (ADR-0008), so the player sees the board again with no input.
          _resumeRoom();
        })
        .catchError((Object e) {
          loginMessage = 'Cannot reach server: $e';
          _onClosed();
        });
  }

  /// Re-enters the room this session was in before the socket dropped.
  void _resumeRoom() {
    final code = _rejoinCode;
    if (code == null) return;
    _rejoinPending = true;
    if (_rejoinSpectating) {
      spectateGame();
    } else {
      joinGame(code);
    }
  }

  /// The automatic rejoin was refused (room dissolved while we were away,
  /// or the credential is no longer good enough). Nothing here is
  /// recoverable without the player, so land them in the menu with the
  /// server's reason rather than on a frozen board.
  void _rejoinFailed(String message) {
    _rejoinPending = false;
    _rejoinCode = null;
    joined = false;
    spectating = false;
    seat = null;
    view = null;
    code = null;
    _resetDirector();
    loginMessage = message;
  }

  /// Schedules the next reconnect attempt, or gives up and hands the
  /// player back to the connect screen.
  void _scheduleReconnect() {
    if (_reconnectAttempts >= _maxReconnectAttempts) {
      _giveUpReconnecting();
      return;
    }
    final delay = _backoff(_reconnectAttempts);
    _reconnectAttempts++;
    reconnecting = true;
    notifyListeners();
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(delay, () {
      if (_disposed || _closedOnPurpose) return;
      _open(_serverUrl);
    });
  }

  void _giveUpReconnecting() {
    _cancelReconnect();
    connected = false;
    joined = false;
    spectating = false;
    view = null;
    _rejoinCode = null;
    _resetDirector();
    loginMessage = 'Disconnected from server.';
    notifyListeners();
  }

  void _cancelReconnect() {
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    reconnecting = false;
    _reconnectAttempts = 0;
  }

  /// The `auth` payload for a create/join/spectate, with the identity token
  /// renewed first when it is near expiry (ADR-0037). This await is the
  /// whole fix for "a long session cannot join another game": the token on
  /// the wire is minted for the message, not for the app's startup.
  Future<Map<String, dynamic>> _auth(String code) async {
    final token = await auth.freshIdToken();
    final name = auth.displayName;
    return {
      if (token != null && token.isNotEmpty) ...{
        'token': token,
        // Chosen in-game handle (ADR-0033); identity stays the token's sub.
        if (name.isNotEmpty) 'display_name': name,
      } else
        'guest_name': name,
      if (_reconnectTokens[code] != null) 'reconnect': _reconnectTokens[code],
    };
  }

  /// Mod ids the connected server can resolve; null until it answers
  /// `list_mods`. Feeds the create-room mod picker so nobody types ids.
  List<String>? availableMods;

  /// Ask the server for its mod ids (connection-scoped, like ping). Cleared
  /// first so the picker shows a fresh loading state per request - and an
  /// old server that ignores the message simply leaves this null, which the
  /// menu degrades to "default mods only".
  void requestMods() {
    availableMods = null;
    _ws?.sink.add(jsonEncode({'type': 'list_mods'}));
  }

  /// Host a new private room. `mods` picks its mod set (ADR-0006).
  ///
  /// Async because the identity token is renewed first if it is close to
  /// expiring; callers fire and forget, the send happens when the payload
  /// is ready.
  Future<void> createGame({List<String> mods = const []}) async {
    final payload = await _auth('');
    _ws?.sink.add(
      jsonEncode({
        'type': 'create',
        'auth': payload,
        if (mods.isNotEmpty) 'mods': mods,
      }),
    );
  }

  /// Join a private room by its 5-letter code.
  Future<void> joinGame(String roomCode) async {
    final c = roomCode.trim().toUpperCase();
    final payload = await _auth(c);
    _ws?.sink.add(jsonEncode({'type': 'join', 'code': c, 'auth': payload}));
  }

  /// Watch a game without playing (ADR-0035). The server picks the room
  /// with the most humans, falling back to its bots showcase.
  Future<void> spectateGame() async {
    final payload = await _auth('');
    _ws?.sink.add(jsonEncode({'type': 'spectate', 'auth': payload}));
  }

  /// Closes the socket deliberately: no reconnect follows.
  void disconnect() {
    _closedOnPurpose = true;
    _cancelReconnect();
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

  /// Leave the room but stay connected, returning to the menu. Deliberate,
  /// so a later socket drop must not resurrect this room.
  void leaveRoom() {
    _ws?.sink.add(jsonEncode({'type': 'leave'}));
    _rejoinCode = null;
    joined = false;
    spectating = false;
    seat = null;
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

  /// Close the connection entirely, returning to the connect screen. The
  /// credential goes with it: a refresh token must not outlive the session
  /// that needed it (ADR-0037).
  void disconnectFromServer() {
    disconnect();
    auth.onChanged = null;
    auth.clear();
    _rejoinCode = null;
    connected = false;
    joined = false;
    spectating = false;
    view = null;
    code = null;
    loginMessage = '';
    notifyListeners();
  }

  /// The socket went away. If nobody asked for that, it is a transport
  /// event - not the end of the session (ADR-0037). The seat is still held
  /// server-side, so retry the socket and let `_resumeRoom` reclaim it;
  /// only an exhausted retry budget sends the player back to sign-in.
  void _onClosed() {
    _sub?.cancel();
    _sub = null;
    _ws = null;
    // A socket that never came up at all is a wrong address or a server
    // that is down - the player needs to see that, not a spinner. Only a
    // connection that was once live is worth retrying.
    if (_disposed || _closedOnPurpose || !connected) {
      _cancelReconnect();
      connected = false;
      joined = false;
      spectating = false;
      view = null;
      _rejoinCode = null;
      _resetDirector();
      if (loginMessage.isEmpty || loginMessage == 'Connecting...') {
        loginMessage = 'Disconnected from server.';
      }
      notifyListeners();
      return;
    }
    // Keep `connected`/`joined` and the last view as they are: the UI
    // stays where the player was (game screen or menu) and shows that it
    // is reconnecting, rather than tearing the room down under them.
    if (loginMessage == 'Connecting...') loginMessage = '';
    _scheduleReconnect();
  }

  /// The tile the last command was about, so a rejection can be shown *on the
  /// thing that refused* rather than as a log line the player is not reading.
  /// The wire carries tile ids (`t3`), not indices; the board speaks indices.
  int? _lastCmdTile;

  void sendCmd(Map<String, dynamic> cmd) {
    final id = cmd['tile'] as String?;
    final i = id == null
        ? -1
        : (content?.board.indexWhere((t) => t.id == id) ?? -1);
    _lastCmdTile = i < 0 ? null : i;
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
    _ws?.sink.add(
      jsonEncode({
        'type': 'feedback',
        'rating': rating,
        if (comment.trim().isNotEmpty) 'comment': comment.trim(),
      }),
    );
    feedbackDone = true;
    notifyListeners();
  }

  void dismissFeedback() {
    feedbackDone = true;
    notifyListeners();
  }

  void _handle(Map<String, dynamic> msg) {
    switch (msg['type']) {
      case 'mods':
        availableMods = (msg['ids'] as List).cast<String>();
      case 'room_created':
        code = msg['code'] as String;
      case 'joined':
        code = msg['code'] as String;
        seat = msg['seat'] as int;
        spectating = false;
        // Remember the room so a dropped socket comes straight back to it.
        _rejoinCode = code;
        _rejoinSpectating = false;
        _rejoinPending = false;
        content = GameContent.fromJson(msg['content'] as Map<String, dynamic>);
        seats = _seatList(msg['players']);
        settings = RoomSettings.fromJson(
          msg['settings'] as Map<String, dynamic>,
        );
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
        if (l10n case final loc?) {
          _log(loc.logJoinedRoom(code ?? '', content!.modIds.join(', ')));
        }
      case 'spectating':
        // Watching without a seat (ADR-0035): same room context as a join,
        // no seat, no reconnect token, no time bank.
        code = msg['code'] as String;
        seat = null;
        spectating = true;
        _rejoinCode = code;
        _rejoinSpectating = true;
        _rejoinPending = false;
        content = GameContent.fromJson(msg['content'] as Map<String, dynamic>);
        seats = _seatList(msg['players']);
        settings = RoomSettings.fromJson(
          msg['settings'] as Map<String, dynamic>,
        );
        if (msg['view'] != null) {
          view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        }
        gameEndsAt = _deadlineFrom(msg['time_remaining']);
        turnSeconds = msg['turn_seconds'] as int?;
        turnEndsAt = _deadlineFrom(_effectiveTurnSeconds());
        timeBankSeconds = null;
        banks = null;
        _resetDirector();
        _trackTimedWindows();
        joined = true;
        loginMessage = '';
        if (l10n case final loc?) _log(loc.logSpectating(code ?? ''));
      case 'lobby':
        final incoming = _seatList(msg['players']);
        _announceSeatChanges(incoming);
        seats = incoming;
        settings = RoomSettings.fromJson(
          msg['settings'] as Map<String, dynamic>,
        );
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
        if (l10n case final loc?) _log(loc.logGameStarted);
      case 'update':
        // Queued rather than applied (ADR-0028): the animation director
        // below plays each Update as paced beats, applies the
        // authoritative view at the end, then acks so the server's gated
        // timers (bid window, turn clock, bot pacing) can start.
        _pendingUpdates.add(msg);
        _drainUpdates();
      case 'rejected':
        sfx.error();
        // On the thing that said no, not in a modal and not only in the log.
        final code = (msg['error'] as Map<String, dynamic>)['code'] as String;
        // Turn the raw engine code into a player-facing reason where we can;
        // the stage carries it for the shaking subject, the feed prints it.
        final loc = l10n;
        final reason = loc != null ? rejectReason(loc, code) : code;
        stage.refuse(_lastCmdTile, reason);
        if (loc != null) _log(loc.logRejected(reason));
      case 'error':
        sfx.error();
        final message = msg['message'] as String;
        // An error answering an automatic rejoin is the one case where the
        // room really is unrecoverable; anything else leaves the room alone.
        if (_rejoinPending) {
          _rejoinFailed(message);
        } else if (!joined) {
          loginMessage = message;
        }
        if (l10n case final loc?) _log(loc.logError(message));
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

  /// What `compile` needs to know about the world (ADR-0030). Positions come
  /// from the *stage*, not the view: a beat animates from where the pawn
  /// visually is, which mid-chain is not where the server says it ended up.
  CompileCtx _ctx() => CompileCtx(
    boardLen: content?.board.length ?? 0,
    jailTile: content?.board.indexWhere((t) => t.kind == 'jail') ?? -1,
    mySeat: seat,
    positions: Map.of(stage.pawnAt),
    tileName: tileName,
    playerName: playerName,
    // The beats that carry text (win / bankruptcy / market / spotlight)
    // localize through this; `l10n` is set every frame, EN is only the
    // pre-first-frame fallback (the same language the beats used to bake).
    loc: l10n ?? AppLocalizationsEn(),
    profile: stage.profile,
  );

  /// Plays one Update: compile the whole burst into a plan, play it, apply the
  /// authoritative view, ack.
  ///
  /// Compiling *before* playing is what makes the budget enforceable (ADR-0030):
  /// the plan's cost is known before the first frame, so an over-budget Update
  /// is compressed rather than discovered to be too long halfway through - at
  /// which point the server would already have un-gated at `ANIM_ACK_CAP` and
  /// left this client behind the game.
  Future<void> _playUpdate(Map<String, dynamic> msg) async {
    final epoch = _updateEpoch;
    final events = (msg['events'] as List).cast<Map<String, dynamic>>();
    if (l10n case final loc?) {
      for (final e in events) {
        _log(
          describeEvent(
            e,
            loc,
            playerName,
            tileName,
            content?.marketEventName ?? (id) => id,
          ),
        );
      }
    }

    if (view != null && joined) {
      await _execute(compile(events, _ctx()), epoch);
      if (_disposed || epoch != _updateEpoch) return; // context changed
    }

    final oldVp = [
      for (final p in view?.players ?? const <PlayerView>[]) p.victoryPoints,
    ];
    view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
    stage.syncPositions([for (final p in view!.players) p.position]);

    // The decision *mode* is derived from the view, not from whatever the beats
    // left behind: an open window keeps its tile lifted and the board receded
    // for as long as the player is still deciding, and everything else clears.
    final turn = view!.turn;
    stage.settle(
      decisionTile: switch (turn.type) {
        'blind_auction' => turn.tile,
        'bribe_vote' => stage.pawnAt[turn.briber ?? -1],
        _ => null,
      },
    );

    // Victory points settle once the authoritative totals land: the aggregate
    // diff covers every source at once (groups won and lost, conglomerates,
    // utilities, the round bonus) without re-deriving the scoring client-side.
    // Gold that moves always means VP - nothing else in the game may.
    for (var i = 0; i < view!.players.length && i < oldVp.length; i++) {
      final delta = view!.players[i].victoryPoints - oldVp[i];
      if (delta == 0) continue;
      stage.addChit(
        from: TileAnchor(stage.pawnAt[i] ?? 0),
        to: SeatAnchor(i),
        text: '${delta > 0 ? '+' : '-'}${delta.abs()} VP',
        kind: ChitKind.victoryPoints,
      );
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

  /// Runs a plan against the clock. Mirrors `Plan.cost` exactly - an exclusive
  /// beat holds the plan open, a concurrent one rides alongside and only costs
  /// whatever of it outlasts the last exclusive beat - so what we waited for is
  /// always what we costed.
  Future<void> _execute(Plan plan, int epoch) async {
    stage.beginPlan();
    var tail = Duration.zero;
    for (final beat in plan.beats) {
      beat.apply(stage);
      if (beat is ArrestBeat) stage.markArrest();
      _sound(beat);
      // A skipped plan still applies every beat - only the waiting stops. State
      // is never lost, only its journey (ADR-0030).
      if (stage.skipping) continue;
      if (beat.lane == Lane.exclusive) {
        tail = Duration.zero;
        if (beat.cost > Duration.zero) {
          await Future<void>.delayed(beat.cost);
          if (_disposed || epoch != _updateEpoch) return;
        }
      } else if (beat.cost > tail) {
        tail = beat.cost;
      }
    }
    if (tail > Duration.zero && !stage.skipping) {
      await Future<void>.delayed(tail);
    }
  }

  /// Sound is a property of the beat's *category*, not of the event that
  /// produced it - one earcon per category, reused everywhere, so a player
  /// learns the vocabulary without being taught it.
  void _sound(Beat beat) {
    switch (beat) {
      case CardPlayBeat():
        sfx.cardPlay();
      case BannerBeat():
        sfx.cardDraw();
      case ChitBeat(:final kind):
        // A third party's money is visible, not audible: sounding every
        // transfer at a six-seat table is a wall of noise. Only what happens
        // to *you* makes a sound.
        if (kind == ChitKind.gain) sfx.gain();
        if (kind == ChitKind.loss) sfx.loss();
      case ArrestBeat():
        sfx.arrest();
      case ThreatBeat():
        sfx.error();
      // Movement sounds per hop from the pawn layer itself; the rest are
      // silent by design - a sound per beat would be a wall of noise.
      case MoveBeat() || JailBeat() || FocusBeat():
      case BidRevealBeat() || BandSweepBeat():
        break;
    }
  }

  /// Aborts any in-flight plan and snaps the stage to truth - called whenever
  /// the room context changes. A reconnecting client renders the present, never
  /// a replay of the past: animating twenty seconds of missed events would
  /// spend its whole budget on history while it is already late for the
  /// decision in front of it.
  void _resetDirector() {
    _updateEpoch++;
    _pendingUpdates.clear();
    _animating = false;
    stage.reset([
      for (final p in view?.players ?? const <PlayerView>[]) p.position,
    ]);
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
    auth.onChanged = null;
    auth.clear();
    super.dispose();
  }
}
