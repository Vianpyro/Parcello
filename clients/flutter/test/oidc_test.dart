// OIDC flow checks: the PKCE transform against the RFC 7636 test vector,
// and the whole login dance against an in-process fake issuer.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/auth_manager.dart';
import 'package:parcello_client/oidc.dart';

void main() {
  test('pkce challenge matches the RFC 7636 appendix B vector', () {
    expect(
      pkceChallenge('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk'),
      'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
    );
  });

  test('jwtDisplayName reads standard claims and survives garbage', () {
    String mint(Map<String, dynamic> claims) {
      final p = base64UrlEncode(utf8.encode(jsonEncode(claims)))
          .replaceAll('=', '');
      return 'h.$p.s';
    }

    expect(jwtDisplayName(mint({'name': 'Vian', 'sub': 'x'})), 'Vian');
    expect(jwtDisplayName(mint({'preferred_username': 'v2', 'sub': 'x'})),
        'v2');
    expect(jwtDisplayName(mint({'sub': 'u_1'})), 'u_1');
    expect(jwtDisplayName('not-a-token'), isNull);
    // Privacy: an email-shaped claim is never surfaced; skip to the next
    // candidate (here the opaque sub).
    expect(
        jwtDisplayName(mint({'name': 'ada@example.com', 'sub': 'u_1'})), 'u_1');
    expect(
        jwtDisplayName(mint({'preferred_username': 'ada@x.com', 'sub': 'u_1'})),
        'u_1');
  });

  test('full PKCE flow against a fake issuer', () async {
    // Fake issuer: discovery + authorize (via the injected browser) + token.
    String? seenVerifier;
    String? issuedCode;
    final issuer = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final base = 'http://127.0.0.1:${issuer.port}';
    issuer.listen((req) async {
      final resp = req.response;
      if (req.uri.path == '/.well-known/openid-configuration') {
        resp.write(jsonEncode({
          'authorization_endpoint': '$base/authorize',
          'token_endpoint': '$base/token',
        }));
      } else if (req.uri.path == '/token') {
        final body = await utf8.decoder.bind(req).join();
        final form = Uri(query: body).queryParameters;
        seenVerifier = form['code_verifier'];
        if (form['code'] == issuedCode) {
          resp.write(jsonEncode({
            'id_token': 'the-id-token',
            'refresh_token': 'the-refresh-token',
            'expires_in': 1800,
          }));
        } else {
          resp.statusCode = 400;
          resp.write('{"error":"invalid_grant"}');
        }
      } else {
        resp.statusCode = 404;
      }
      await resp.close();
    });

    // "Browser": parse the authorize URL and immediately hit the loopback
    // redirect with a code, like a user who approved the login.
    Future<void> fakeBrowser(String url) async {
      final u = Uri.parse(url);
      expect(u.queryParameters['code_challenge_method'], 'S256');
      expect(u.queryParameters['client_id'], 'parcello');
      // Rauthy grants a refresh token without being asked, but issuers
      // that gate on this scope would not - and a session with no refresh
      // token dies at `exp` (ADR-0037).
      expect(u.queryParameters['scope'], contains('offline_access'));
      issuedCode = 'code-123';
      final redirect = Uri.parse(u.queryParameters['redirect_uri']!).replace(
        queryParameters: {
          'code': issuedCode,
          'state': u.queryParameters['state'],
        },
      );
      // Fire the callback without awaiting its response: a real browser is
      // a separate process; awaiting here would deadlock loginWithOidc,
      // which only starts serving the loopback after openUrl returns.
      unawaited(() async {
        final client = HttpClient();
        try {
          final cb = await client.getUrl(redirect);
          await (await cb.close()).drain<void>();
        } finally {
          client.close();
        }
      }());
    }

    final tokens =
        await loginWithOidc(base, 'parcello', openUrl: fakeBrowser);
    expect(tokens.idToken, 'the-id-token');
    // The whole grant is kept, not just the id_token: the refresh token is
    // what lets a session outlive the token's lifetime (ADR-0037).
    expect(tokens.refreshToken, 'the-refresh-token');
    expect(tokens.expiresAt, isNotNull);
    expect(seenVerifier, isNotNull, reason: 'PKCE verifier must be sent');
    await issuer.close(force: true);
  });

  group('token lifecycle (ADR-0037)', () {
    /// A JWT whose only interesting claim is `exp`, `inSeconds` from now.
    String mintExpiring(int inSeconds, {String sub = 'u_1'}) {
      final exp = DateTime.now()
              .add(Duration(seconds: inSeconds))
              .millisecondsSinceEpoch ~/
          1000;
      final payload = base64UrlEncode(
              utf8.encode(jsonEncode({'sub': sub, 'exp': exp})))
          .replaceAll('=', '');
      return 'h.$payload.s';
    }

    test('expiry comes from the id_token exp, not expires_in', () {
      // The server checks the id_token's `exp` (eddsa.rs); `expires_in`
      // describes the access token and can differ. Trusting the wrong one
      // means renewing after the credential is already dead.
      final idToken = mintExpiring(1800);
      final tokens = OidcTokens.fromResponse(jsonEncode({
        'id_token': idToken,
        'refresh_token': 'r1',
        'expires_in': 30,
      }));
      final remaining = tokens.expiresAt!.difference(DateTime.now());
      expect(remaining.inSeconds, greaterThan(1700));
    });

    test('expires_in is the fallback when the token carries no exp', () {
      final tokens = OidcTokens.fromResponse(
          jsonEncode({'id_token': 'h.e30.s', 'expires_in': 600}));
      expect(tokens.expiresAt!.difference(DateTime.now()).inSeconds,
          closeTo(600, 5));
    });

    test('a refresh that omits refresh_token keeps the previous one', () {
      final tokens = OidcTokens.fromResponse(
          jsonEncode({'id_token': mintExpiring(1800)}),
          previousRefresh: 'still-valid');
      expect(tokens.refreshToken, 'still-valid');
    });

    test('a grant with no id_token is refused', () {
      expect(() => OidcTokens.fromResponse(jsonEncode({'access_token': 'a'})),
          throwsA(isA<OidcNoIdToken>()));
    });

    test('an issuer that will not reissue an ID token stops, not retries',
        () async {
      // OIDC Core 12.2 makes `id_token` OPTIONAL in a refresh response.
      // Parcello authenticates with the ID token (ADR-0009 amendment 2),
      // so such an issuer simply cannot renew - and must be reported as
      // that, rather than being retried every 30s until the heat death.
      var attempts = 0;
      final issuer = await _fakeIssuer(onRefresh: () {
        attempts++;
        return {'access_token': 'a', 'refresh_token': 'rotated'};
      });
      final auth = AuthManager(
        displayName: 'vian',
        tokens: OidcTokens(
            idToken: 'dead-token',
            refreshToken: 'r1',
            expiresAt: DateTime.now().subtract(const Duration(seconds: 5))),
        issuer: issuer.base,
        clientId: 'parcello',
      );

      await auth.freshIdToken();
      expect(attempts, 1);
      expect(auth.cannotRenew, isTrue,
          reason: 'the deployment fact must be diagnosable');
      expect(auth.signInRequired, isTrue);
      auth.clear();
      expect(auth.cannotRenew, isFalse);
      await issuer.close();
    });

    test('a guest has no token and nothing to renew', () async {
      final auth = AuthManager.guest('vian');
      expect(auth.hasToken, isFalse);
      expect(auth.canRenew, isFalse);
      expect(await auth.freshIdToken(), isNull);
    });

    test('a fresh token is used as-is - no needless refresh', () async {
      var refreshes = 0;
      final issuer = await _fakeIssuer(onRefresh: () {
        refreshes++;
        return {'id_token': mintExpiring(1800), 'refresh_token': 'r2'};
      });
      final auth = AuthManager(
        displayName: 'vian',
        tokens: OidcTokens(
            idToken: 'live-token',
            refreshToken: 'r1',
            expiresAt: DateTime.now().add(const Duration(seconds: 1800))),
        issuer: issuer.base,
        clientId: 'parcello',
      );
      expect(await auth.freshIdToken(), 'live-token');
      expect(refreshes, 0, reason: 'a healthy token must not be renewed');
      auth.clear();
      await issuer.close();
    });

    test('a near-expiry token renews before it is sent, once', () async {
      var refreshes = 0;
      String? sentRefreshToken;
      final renewed = mintExpiring(1800);
      final issuer = await _fakeIssuer(onRefresh: () {
        refreshes++;
        return {'id_token': renewed, 'refresh_token': 'rotated'};
      }, capture: (form) => sentRefreshToken = form['refresh_token']);

      final auth = AuthManager(
        displayName: 'vian',
        // Already past exp: exactly the state a session reached at 1800s.
        tokens: OidcTokens(
            idToken: 'dead-token',
            refreshToken: 'r1',
            expiresAt: DateTime.now().subtract(const Duration(seconds: 5))),
        issuer: issuer.base,
        clientId: 'parcello',
      );

      // Two concurrent callers (a join racing the reconnect) share one
      // round trip rather than burning the rotated token twice.
      final results =
          await Future.wait([auth.freshIdToken(), auth.freshIdToken()]);
      expect(results, [renewed, renewed]);
      expect(refreshes, 1, reason: 'refresh must be single-flight');
      expect(sentRefreshToken, 'r1');
      expect(auth.signInRequired, isFalse);

      // Rotation is honoured: the next renewal uses the NEW refresh token.
      expect(await auth.freshIdToken(), renewed);
      auth.clear();
      await issuer.close();
    });

    test('a refused refresh is terminal and asks for a new sign-in',
        () async {
      final issuer = await _fakeIssuer(refuse: true);
      final auth = AuthManager(
        displayName: 'vian',
        tokens: OidcTokens(
            idToken: 'dead-token',
            refreshToken: 'revoked',
            expiresAt: DateTime.now().subtract(const Duration(seconds: 5))),
        issuer: issuer.base,
        clientId: 'parcello',
      );
      await auth.freshIdToken();
      expect(auth.signInRequired, isTrue);
      auth.clear();
      await issuer.close();
    });

    test('an expired token with no refresh token asks for a new sign-in',
        () async {
      // What an issuer that disallows offline_access leaves us with.
      final auth = AuthManager(
        displayName: 'vian',
        tokens: OidcTokens(
            idToken: 'dead-token',
            expiresAt: DateTime.now().subtract(const Duration(seconds: 5))),
        issuer: 'http://127.0.0.1:1',
        clientId: 'parcello',
      );
      expect(auth.canRenew, isFalse);
      await auth.freshIdToken();
      expect(auth.signInRequired, isTrue);
      auth.clear();
    });

    test('clear() drops the refresh token', () async {
      final auth = AuthManager(
        displayName: 'vian',
        tokens: const OidcTokens(idToken: 'a', refreshToken: 'r'),
        issuer: 'https://auth.example.com',
        clientId: 'parcello',
      );
      expect(auth.hasToken, isTrue);
      auth.clear();
      expect(auth.hasToken, isFalse);
      expect(auth.canRenew, isFalse);
      expect(await auth.freshIdToken(), isNull);
    });
  });
}

/// Minimal OIDC issuer for the refresh tests: discovery + token endpoint.
class _FakeIssuer {
  final HttpServer server;
  final String base;
  _FakeIssuer(this.server, this.base);
  Future<void> close() => server.close(force: true);
}

Future<_FakeIssuer> _fakeIssuer({
  Map<String, dynamic> Function()? onRefresh,
  void Function(Map<String, String> form)? capture,
  bool refuse = false,
}) async {
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
      final body = await utf8.decoder.bind(req).join();
      final form = Uri(query: body).queryParameters;
      capture?.call(form);
      if (refuse) {
        resp.statusCode = 400;
        resp.write('{"error":"invalid_grant"}');
      } else {
        resp.write(jsonEncode(onRefresh!()));
      }
    } else {
      resp.statusCode = 404;
    }
    await resp.close();
  });
  return _FakeIssuer(server, base);
}
