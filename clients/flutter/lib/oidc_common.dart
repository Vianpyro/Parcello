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

/// Claims of an (unverified) JWT payload, or null if it is not a JWT at all.
/// Decoding without verifying is safe for the two things we do with it -
/// pick a display name, and read the expiry we schedule our own refresh
/// against. The server is the only party that trusts a token (ADR-0009).
Map<String, dynamic>? jwtClaims(String token) {
  try {
    final payload = token.split('.')[1];
    final claims = jsonDecode(
            utf8.decode(base64Url.decode(base64Url.normalize(payload))))
        as Map<String, dynamic>;
    return claims;
  } catch (_) {
    return null;
  }
}

/// Display name from an (unverified) JWT payload - UI hint only, the
/// server does the real verification. Mirrors the server's privacy guard
/// (server eddsa.rs `display_name`): an email-shaped claim (`@`) is skipped so the
/// account address is never surfaced, even to the player themselves.
String? jwtDisplayName(String token) {
  final claims = jwtClaims(token);
  if (claims == null) return null;
  bool ok(Object? v) => v is String && v.trim().isNotEmpty && !v.contains('@');
  return [claims['name'], claims['preferred_username'], claims['sub']]
      .firstWhere(ok, orElse: () => null) as String?;
}

/// The `exp` claim as a wall-clock instant, or null when absent/malformed.
/// This is the deadline the game server enforces (`eddsa.rs`), so it - not
/// the token response's `expires_in`, which describes the *access* token -
/// is what the client schedules its renewal against (ADR-0037).
DateTime? jwtExpiry(String token) {
  final exp = jwtClaims(token)?['exp'];
  if (exp is! num) return null;
  return DateTime.fromMillisecondsSinceEpoch(exp.toInt() * 1000, isUtc: true)
      .toLocal();
}

class OidcEndpoints {
  final String authorization;
  final String token;
  OidcEndpoints(this.authorization, this.token);
}

/// Scopes every login asks for (ADR-0037).
///
/// `offline_access` is the standard way to ask for a refresh token. The
/// reference issuer (Rauthy) grants one without being asked, so this is
/// portability rather than necessity: Keycloak, Zitadel and Authentik do
/// gate refresh tokens on this scope, and requesting it costs nothing
/// where it is already granted. An issuer that refuses it returns a grant
/// with no refresh token and the session simply ends at `exp`.
const oidcScopes = 'openid profile offline_access';

/// One grant from the token endpoint: the id_token the game server
/// verifies, the refresh token that renews it, and when the id_token dies.
///
/// The refresh token is a long-lived bearer credential and is deliberately
/// never persisted - it lives here, in memory, for the run of the app
/// (ADR-0037; same posture ADR-0009 chose for the id_token).
class OidcTokens {
  final String idToken;

  /// Null when the issuer granted none. The session then cannot be
  /// renewed and ends at [expiresAt].
  final String? refreshToken;

  /// When [idToken] stops verifying server-side. Null only for a token
  /// with no `exp`, which the server would refuse anyway.
  final DateTime? expiresAt;

  const OidcTokens({
    required this.idToken,
    this.refreshToken,
    this.expiresAt,
  });

  /// Projects a token-endpoint response. `previousRefresh` is carried
  /// forward when the issuer rotates nothing back (RFC 6749 allows
  /// omitting `refresh_token` on a refresh; Rauthy rotates, so usually
  /// there is a new one to take).
  ///
  /// Throws [OidcNoIdToken] when the response carries none - without it
  /// there is nothing the game server can verify (ADR-0009 amendment 2).
  /// On the initial code exchange that means a misconfigured client; on a
  /// refresh it means an issuer that does not renew ID tokens, which
  /// OIDC Core section 12.2 explicitly permits ("...except that it might
  /// not contain an id_token"). Either way it is terminal, not transient.
  factory OidcTokens.fromResponse(String body, {String? previousRefresh}) {
    final json = jsonDecode(body) as Map<String, dynamic>;
    final idToken = json['id_token'] as String?;
    if (idToken == null) throw const OidcNoIdToken();
    // `expires_in` is about the ACCESS token; the id_token's own `exp` is
    // the deadline the game server enforces. Prefer it, fall back only if
    // the token carries no exp at all.
    final expiresIn = json['expires_in'];
    return OidcTokens(
      idToken: idToken,
      refreshToken: (json['refresh_token'] as String?) ?? previousRefresh,
      expiresAt: jwtExpiry(idToken) ??
          (expiresIn is num
              ? DateTime.now().add(Duration(seconds: expiresIn.toInt()))
              : null),
    );
  }
}

/// Thrown when a grant carries no `id_token`.
///
/// Parcello authenticates to the game server with the ID token (ADR-0009,
/// amendment 2), so a grant without one is unusable. On a *refresh* this
/// is a legitimate issuer behaviour - OIDC Core section 12.2 makes the
/// `id_token` optional in a refresh response - and it means this issuer
/// cannot renew a Parcello session at all. Terminal either way: retrying
/// produces the same answer, so the player is asked to sign in rather than
/// left in a retry loop.
class OidcNoIdToken implements Exception {
  const OidcNoIdToken();
  @override
  String toString() =>
      'the identity provider returned no id_token '
      '(if this happens on renewal, its OIDC client does not reissue ID '
      'tokens on refresh)';
}

/// Thrown when the issuer refuses a refresh outright (`invalid_grant`: the
/// token was revoked, expired, or already consumed). Terminal - retrying
/// the same credential cannot succeed, so the caller must sign in again
/// rather than back off and try harder (ADR-0037).
class OidcRefreshRejected implements Exception {
  final String message;
  const OidcRefreshRejected(this.message);
  @override
  String toString() => message;
}

/// Redeems a refresh token for a new grant (RFC 6749 section 6). Public
/// client: `client_id`, no secret.
///
/// Throws [OidcRefreshRejected] when the issuer says the grant itself is
/// dead, and a plain `String` for anything transient (network, 5xx) so the
/// caller can retry those.
Future<OidcTokens> refreshTokens(
  String tokenEndpoint,
  String clientId,
  String refreshToken,
) async {
  final resp = await http.post(Uri.parse(tokenEndpoint), body: {
    'grant_type': 'refresh_token',
    'refresh_token': refreshToken,
    'client_id': clientId,
  });
  if (resp.statusCode == 200) {
    return OidcTokens.fromResponse(resp.body, previousRefresh: refreshToken);
  }
  // 4xx from the token endpoint is a verdict on the credential, not a
  // hiccup: OAuth error responses are 400/401 (RFC 6749 section 5.2).
  if (resp.statusCode >= 400 && resp.statusCode < 500) {
    throw OidcRefreshRejected('refresh refused: ${resp.body}');
  }
  throw 'token refresh failed: ${resp.statusCode}';
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
