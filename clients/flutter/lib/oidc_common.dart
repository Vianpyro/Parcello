/// Portable OIDC pieces shared by the native (`oidc_login_io.dart`) and web
/// (`oidc_login_web.dart`) login flows: PKCE/JWT utilities plus discovery
/// and token exchange over `package:http` (auto-selects a native or
/// browser-safe client per platform).
library;

import 'dart:convert';
import 'dart:math';

import 'package:crypto/crypto.dart';
import 'package:http/http.dart' as http;

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
  final resp =
      await http.get(Uri.parse('$base/.well-known/openid-configuration'));
  if (resp.statusCode != 200) {
    throw 'discovery failed: ${resp.statusCode}';
  }
  final doc = jsonDecode(resp.body) as Map<String, dynamic>;
  return OidcEndpoints(
    doc['authorization_endpoint'] as String,
    doc['token_endpoint'] as String,
  );
}

/// Form-encoded POST (the OIDC token endpoint's required content type).
Future<String> exchangeToken(
    String tokenEndpoint, Map<String, String> form) async {
  final resp = await http.post(Uri.parse(tokenEndpoint), body: form);
  if (resp.statusCode != 200) throw 'token exchange failed: ${resp.body}';
  return resp.body;
}
