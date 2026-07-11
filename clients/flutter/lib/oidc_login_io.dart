/// Native OIDC login (ADR-0009; Rauthy is the reference deployment):
/// system browser + loopback redirect, public client (PKCE instead of a
/// client secret). Returns the raw EdDSA id_token; the game server is the
/// one that verifies it. The token is kept in memory only - never written
/// to disk (privacy over convenience; they expire within a day anyway).
///
/// Selected for non-web targets by the conditional export in `oidc.dart`.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'oidc_common.dart';

export 'oidc_common.dart';

/// Runs the full login flow. `openUrl` defaults to the system browser and
/// is injectable for tests. Throws a `String` message on failure.
Future<String> loginWithOidc(
  String issuer,
  String clientId, {
  Future<void> Function(String url)? openUrl,
}) async {
  final endpoints = await discover(issuer);
  final verifier = randomUrlSafe(48);
  final state = randomUrlSafe(16);

  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  try {
    final redirect = 'http://127.0.0.1:${server.port}/cb';
    final authUrl =
        Uri.parse(endpoints.authorization).replace(queryParameters: {
      'client_id': clientId,
      'redirect_uri': redirect,
      'response_type': 'code',
      'scope': 'openid profile',
      'state': state,
      'code_challenge': pkceChallenge(verifier),
      'code_challenge_method': 'S256',
    });
    await (openUrl ?? _openBrowser)(authUrl.toString());

    final request =
        await server.first.timeout(const Duration(minutes: 5), onTimeout: () {
      throw 'login timed out (no browser callback)';
    });
    final params = request.uri.queryParameters;
    request.response
      ..headers.contentType = ContentType.html
      ..write('<h2>Parcello: signed in. You can close this tab.</h2>');
    await request.response.close();

    if (params['state'] != state) throw 'login state mismatch';
    final code = params['code'];
    if (code == null) {
      throw 'login refused: ${params['error'] ?? 'no code returned'}';
    }

    final body = await exchangeToken(endpoints.token, {
      'grant_type': 'authorization_code',
      'code': code,
      'redirect_uri': redirect,
      'client_id': clientId,
      'code_verifier': verifier,
    });
    final idToken =
        (jsonDecode(body) as Map<String, dynamic>)['id_token'] as String?;
    if (idToken == null) throw 'identity provider returned no id_token';
    return idToken;
  } finally {
    await server.close(force: true);
  }
}

Future<void> _openBrowser(String url) async {
  final (cmd, args) = switch (Platform.operatingSystem) {
    'windows' => ('rundll32', ['url.dll,FileProtocolHandler', url]),
    'macos' => ('open', [url]),
    _ => ('xdg-open', [url]),
  };
  await Process.run(cmd, args);
}
