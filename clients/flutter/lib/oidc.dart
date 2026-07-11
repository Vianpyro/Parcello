/// OIDC Authorization Code + PKCE login against the identity provider
/// (ADR-0009; Rauthy is the reference deployment). Public client (PKCE
/// instead of a client secret). Returns the raw EdDSA id_token; the game
/// server is the one that verifies it. The token is kept in memory only -
/// never written to disk (privacy over convenience; they expire within a
/// day anyway).
///
/// `dart:io` (the native loopback-redirect flow) doesn't exist on the web
/// compile target, so the actual login implementation is chosen at compile
/// time: `oidc_login_io.dart` (system browser + loopback server) normally,
/// `oidc_login_web.dart` (popup + postMessage) when compiling for web.
/// Both re-export `oidc_common.dart`'s portable PKCE/JWT/discovery helpers,
/// so this file's public API - `loginWithOidc`, `pkceChallenge`,
/// `randomUrlSafe`, `jwtDisplayName`, `OidcEndpoints`, `discover` - is
/// identical on every platform.
library;

export 'oidc_login_io.dart' if (dart.library.js_interop) 'oidc_login_web.dart';
