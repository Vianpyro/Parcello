/// Connection + game state in one ChangeNotifier: the Dart equivalent of the
/// reference web client's `st` object and message switch. The server is
/// authoritative; this only projects what it pushes.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'protocol.dart';

class GameSession extends ChangeNotifier {
  WebSocketChannel? _ws;

  int? seat;
  String? code;
  GameContent? content;
  ClientView? view;
  List<SeatInfo> seats = [];
  final List<String> log = [];
  String loginMessage = '';
  bool joined = false;

  bool get myTurn => view != null && seat != null && view!.current == seat;

  String playerName(int i) =>
      view?.players.elementAtOrNull(i)?.name ??
      seats.elementAtOrNull(i)?.name ??
      'seat $i';

  String tileName(int i) => content?.board.elementAtOrNull(i)?.name ?? 'tile $i';

  /// Opens the socket and immediately creates or joins a room.
  void connect(String url, String name, String roomCode) {
    disconnect();
    loginMessage = 'Connecting...';
    notifyListeners();
    try {
      _ws = WebSocketChannel.connect(Uri.parse(url));
    } catch (e) {
      loginMessage = 'Bad server URL: $e';
      notifyListeners();
      return;
    }
    final auth = {'guest_name': name};
    _ws!.sink.add(jsonEncode(roomCode.isEmpty
        ? {'type': 'create', 'auth': auth}
        : {'type': 'join', 'code': roomCode, 'auth': auth}));
    _ws!.stream.listen(
      (data) => _handle(jsonDecode(data as String) as Map<String, dynamic>),
      onDone: _onClosed,
      onError: (Object e) {
        loginMessage = 'Connection failed: $e';
        _onClosed();
      },
    );
  }

  void disconnect() {
    _ws?.sink.close();
    _ws = null;
  }

  void _onClosed() {
    _ws = null;
    joined = false;
    if (loginMessage == 'Connecting...' || loginMessage.isEmpty) {
      loginMessage = 'Disconnected. Enter the room code to rejoin.';
    }
    notifyListeners();
  }

  void sendCmd(Map<String, dynamic> cmd) {
    _ws?.sink.add(jsonEncode({'type': 'cmd', 'cmd': cmd}));
  }

  void sendStart() {
    _ws?.sink.add(jsonEncode({'type': 'start'}));
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
        if (msg['view'] != null) {
          view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        }
        joined = true;
        loginMessage = '';
        _log('Joined room $code. Mods: ${content!.modIds.join(', ')}');
      case 'lobby':
        seats = _seatList(msg['players']);
      case 'game_started':
        view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        _log('Game started.');
      case 'update':
        view = ClientView.fromJson(msg['view'] as Map<String, dynamic>);
        for (final e in msg['events'] as List) {
          _log(describeEvent(
              e as Map<String, dynamic>, playerName, tileName));
        }
      case 'rejected':
        _log('Rejected: ${msg['error']['code']}');
      case 'error':
        if (!joined) loginMessage = msg['message'] as String;
        _log('Error: ${msg['message']}');
    }
    notifyListeners();
  }

  List<SeatInfo> _seatList(dynamic players) => (players as List)
      .map((s) => SeatInfo.fromJson(s as Map<String, dynamic>))
      .toList();

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
