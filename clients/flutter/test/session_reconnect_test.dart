/// Transparent session recovery (ADR-0037), end to end over a real local
/// WebSocket: a socket that drops mid-game must come back by itself, and
/// the rejoin it sends must carry a token that is valid *now* - not the one
/// minted when the app started.
///
/// This is the regression surface for the two reported symptoms: a player
/// disconnected mid-match at the token's lifetime, and a long session that
/// could no longer join another game.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/auth_manager.dart';
import 'package:parcello_client/oidc.dart';
import 'package:parcello_client/session.dart';

/// A `joined` reply in the lobby phase: enough shape for the client to
/// accept it, no view (the game has not started).
Map<String, dynamic> _joined(String code) => {
      'type': 'joined',
      'code': code,
      'seat': 0,
      'players': [
        {
          'seat': 0,
          'player_id': 'id:u_1',
          'name': 'Vian',
          'connected': true,
          'is_bot': false,
        }
      ],
      'content': {
        'mods': [
          {'id': 'base'}
        ],
        'content': {
          'board': [
            {
              'id': 't0',
              'name': 'Go',
              'kind': {'type': 'go'},
            }
          ],
          'rules': {'starting_balance': 1500, 'go_salary': 200},
          'market_events': <dynamic>[],
        },
      },
      'settings': {
        'rules': {
          'starting_balance': 1500,
          'go_salary': 200,
          'max_houses_per_property': 5,
          'bankruptcy_threshold': 0,
        },
      },
    };

/// Fake game server: accepts sockets, records every auth payload it is
/// sent, and can hang up on demand the way a proxy idle-timeout does.
class _FakeServer {
  final HttpServer http;
  final List<Map<String, dynamic>> joins = [];
  final List<WebSocket> sockets = [];

  _FakeServer(this.http);

  String get url => 'ws://127.0.0.1:${http.port}/ws';

  static Future<_FakeServer> start() async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final fake = _FakeServer(server);
    server.listen((req) async {
      final ws = await WebSocketTransformer.upgrade(req);
      fake.sockets.add(ws);
      ws.listen((data) {
        final msg = jsonDecode(data as String) as Map<String, dynamic>;
        if (msg['type'] == 'join') {
          fake.joins.add(msg);
          ws.add(jsonEncode(_joined(msg['code'] as String)));
        }
      });
    });
    return fake;
  }

  /// Drop the live socket without a close handshake, as a proxy would.
  Future<void> hangUp() async {
    for (final ws in sockets) {
      await ws.close();
    }
    sockets.clear();
  }

  Future<void> close() => http.close(force: true);
}

/// Polls until [check] holds or the budget runs out - the reconnect is
/// driven by real timers, so there is nothing to await directly.
Future<void> waitFor(bool Function() check, {String? because}) async {
  for (var i = 0; i < 200; i++) {
    if (check()) return;
    await Future<void>.delayed(const Duration(milliseconds: 25));
  }
  fail(because ?? 'condition never held');
}

void main() {
  test('a dropped socket reconnects and re-enters the room by itself',
      () async {
    final server = await _FakeServer.start();
    final session = GameSession()
      ..connect(server.url, AuthManager.guest('vian'));

    await waitFor(() => session.connected, because: 'socket never came up');
    await session.joinGame('ABCDE');
    await waitFor(() => session.joined, because: 'never joined');
    expect(server.joins.length, 1);

    // The proxy hangs up mid-game.
    await server.hangUp();

    // The player stays in the room - no trip back to the sign-in screen -
    // and the client re-enters the same room on its own.
    await waitFor(() => server.joins.length == 2,
        because: 'the client never rejoined after the drop');
    expect(server.joins[1]['code'], 'ABCDE');
    expect(session.joined, isTrue, reason: 'the room must survive the drop');
    await waitFor(() => !session.reconnecting);
    expect(session.connected, isTrue);

    session.dispose();
    await server.close();
  });

  test('the rejoin after a drop carries a freshly renewed token', () async {
    // A token already past its expiry - exactly the state a session was in
    // at the issuer's configured lifetime.
    final renewed = _mintExpiring(1800);
    var refreshes = 0;
    final issuer = await _fakeIssuer(() {
      refreshes++;
      return {'id_token': renewed, 'refresh_token': 'rotated'};
    });

    final server = await _FakeServer.start();
    final session = GameSession()
      ..connect(
        server.url,
        AuthManager(
          displayName: 'Vian',
          tokens: OidcTokens(
            idToken: 'stale-token',
            refreshToken: 'r1',
            expiresAt: DateTime.now().subtract(const Duration(seconds: 1)),
          ),
          issuer: issuer.$1,
          clientId: 'parcello',
        ),
      );

    await waitFor(() => session.connected);
    await session.joinGame('ABCDE');
    await waitFor(() => session.joined);

    // Even the FIRST join renews: the stale token would have been refused.
    expect(server.joins[0]['auth']['token'], renewed);
    expect(server.joins[0]['auth']['display_name'], 'Vian');
    expect(refreshes, 1);

    await server.hangUp();
    await waitFor(() => server.joins.length == 2,
        because: 'the client never rejoined');
    expect(server.joins[1]['auth']['token'], renewed,
        reason: 'the rejoin must present a valid credential');
    // The renewed token is still healthy, so no second round trip.
    expect(refreshes, 1, reason: 'no needless refresh');

    session.dispose();
    await server.close();
    await issuer.$2.close(force: true);
  });

  test('a deliberate leave is not undone by the reconnect logic', () async {
    final server = await _FakeServer.start();
    final session = GameSession()
      ..connect(server.url, AuthManager.guest('vian'));

    await waitFor(() => session.connected);
    await session.joinGame('ABCDE');
    await waitFor(() => session.joined);

    session.leaveRoom();
    await server.hangUp();

    // The socket comes back (the player is still on this server) but must
    // NOT drag them back into the room they just left.
    await waitFor(() => session.connected && !session.reconnecting);
    await Future<void>.delayed(const Duration(milliseconds: 200));
    expect(server.joins.length, 1, reason: 'a left room must not be rejoined');
    expect(session.joined, isFalse);

    session.dispose();
    await server.close();
  });

  test('disconnecting from the server stops reconnection and drops the token',
      () async {
    final server = await _FakeServer.start();
    final auth = AuthManager(
      displayName: 'Vian',
      tokens: const OidcTokens(idToken: 'tok', refreshToken: 'r1'),
      issuer: 'https://auth.example.com',
      clientId: 'parcello',
    );
    final session = GameSession()..connect(server.url, auth);
    await waitFor(() => session.connected);

    session.disconnectFromServer();
    expect(session.connected, isFalse);
    expect(auth.hasToken, isFalse,
        reason: 'the refresh token must not outlive the session');

    await Future<void>.delayed(const Duration(milliseconds: 300));
    expect(session.reconnecting, isFalse);

    session.dispose();
    await server.close();
  });

  test('an unreachable server fails instead of retrying forever', () async {
    // Nothing is listening: the socket never comes up, so this is a wrong
    // address - the player must be told, not shown a spinner.
    final session = GameSession()
      ..connect('ws://127.0.0.1:1/ws', AuthManager.guest('vian'));

    await waitFor(() => session.loginMessage.isNotEmpty,
        because: 'a failed first connect must surface a message');
    expect(session.connected, isFalse);
    expect(session.reconnecting, isFalse);
    session.dispose();
  });
}

String _mintExpiring(int inSeconds) {
  final exp =
      DateTime.now().add(Duration(seconds: inSeconds)).millisecondsSinceEpoch ~/
          1000;
  final payload =
      base64UrlEncode(utf8.encode(jsonEncode({'sub': 'u_1', 'exp': exp})))
          .replaceAll('=', '');
  return 'h.$payload.s';
}

/// Discovery + token endpoint; returns (issuer base, server).
Future<(String, HttpServer)> _fakeIssuer(
    Map<String, dynamic> Function() onRefresh) async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  final base = 'http://127.0.0.1:${server.port}';
  server.listen((req) async {
    final resp = req.response;
    if (req.uri.path == '/.well-known/openid-configuration') {
      resp.write(jsonEncode({
        'authorization_endpoint': '$base/authorize',
        'token_endpoint': '$base/token',
      }));
    } else if (req.uri.path == '/token') {
      await utf8.decoder.bind(req).join();
      resp.write(jsonEncode(onRefresh()));
    } else {
      resp.statusCode = 404;
    }
    await resp.close();
  });
  return (base, server);
}
