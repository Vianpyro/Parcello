/// OIDC Authorization Code + PKCE login against the identity provider
/// (ADR-0009; Rauthy is the reference deployment). Native-app pattern:
/// system browser + loopback redirect, public client (PKCE instead of a
/// client secret). Returns the raw EdDSA id_token; the game server is the
/// one that verifies it. The token is kept in memory only - never written
/// to disk (privacy over convenience; they expire within a day anyway).
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:crypto/crypto.dart';

/// RFC 7636 S256: BASE64URL(SHA256(ascii(verifier))), no padding.
String pkceChallenge(String verifier) =>
    base64UrlEncode(sha256.convert(ascii.encode(verifier)).bytes)
        .replaceAll('=', '');

String randomUrlSafe(int bytes) {
  final rng = Random.secure();
  return base64UrlEncode(List.generate(bytes, (_) => rng.nextInt(256)))
      .replaceAll('=', '');
}

/// Display name from an (unverified) JWT payload - UI hint only, the
/// server does the real verification.
String? jwtDisplayName(String token) {
  try {
    final payload = token.split('.')[1];
    final claims = jsonDecode(
            utf8.decode(base64Url.decode(base64Url.normalize(payload))))
        as Map<String, dynamic>;
    return (claims['name'] ?? claims['preferred_username'] ?? claims['sub'])
        as String?;
  } catch (_) {
    return null;
  }
}

class OidcEndpoints {
  final String authorization;
  final String token;
  OidcEndpoints(this.authorization, this.token);
}

Future<OidcEndpoints> discover(String issuer) async {
  final base = issuer.replaceAll(RegExp(r'/+$'), '');
  final body = await _get('$base/.well-known/openid-configuration');
  final doc = jsonDecode(body) as Map<String, dynamic>;
  return OidcEndpoints(
    doc['authorization_endpoint'] as String,
    doc['token_endpoint'] as String,
  );
}

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

    final body = await _post(endpoints.token, {
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

Future<String> _get(String url) async {
  final client = HttpClient();
  try {
    final req = await client.getUrl(Uri.parse(url));
    final resp = await req.close();
    final body = await resp.transform(utf8.decoder).join();
    if (resp.statusCode != 200) throw 'GET $url failed: ${resp.statusCode}';
    return body;
  } finally {
    client.close();
  }
}

Future<String> _post(String url, Map<String, String> form) async {
  final client = HttpClient();
  try {
    final req = await client.postUrl(Uri.parse(url));
    req.headers.contentType =
        ContentType('application', 'x-www-form-urlencoded');
    req.write(Uri(queryParameters: form).query);
    final resp = await req.close();
    final body = await resp.transform(utf8.decoder).join();
    if (resp.statusCode != 200) throw 'token exchange failed: $body';
    return body;
  } finally {
    client.close();
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
