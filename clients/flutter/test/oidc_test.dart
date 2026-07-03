// OIDC flow checks: the PKCE transform against the RFC 7636 test vector,
// and the whole login dance against an in-process fake issuer.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
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
          resp.write(jsonEncode({'id_token': 'the-id-token'}));
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

    final token =
        await loginWithOidc(base, 'parcello', openUrl: fakeBrowser);
    expect(token, 'the-id-token');
    expect(seenVerifier, isNotNull, reason: 'PKCE verifier must be sent');
    await issuer.close(force: true);
  });
}
