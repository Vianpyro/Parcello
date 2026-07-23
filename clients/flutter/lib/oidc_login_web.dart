/// Web OIDC login: popup + `postMessage` (no loopback listener - a browser
/// page cannot bind a local port). The popup is opened synchronously,
/// before any `await`, so it stays inside the caller's user-gesture chain
/// and isn't treated as an unsolicited popup; it starts blank and is
/// navigated to the real authorization URL once discovery completes.
///
/// Returns the whole grant (id_token + refresh token, ADR-0037), in memory
/// only - `localStorage` is readable by any script that reaches this
/// origin, so a refresh token must never land there.
///
/// `web/oidc-callback.html` is the registered `redirect_uri` for this
/// origin - it forwards `code`/`state`/`error` back via `postMessage` and
/// closes itself. The identity provider's OIDC client must have this
/// origin's callback URL registered alongside the native loopback one
/// (docs/deployment.md).
///
/// Selected for the web target by the conditional export in `oidc.dart`.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';

import 'package:web/web.dart' as web;

import 'oidc_common.dart';

export 'oidc_common.dart';

/// Runs the full login flow. `openUrl`, if given, replaces the popup with
/// a caller-supplied navigation (kept for signature parity with the native
/// flow; real web callers should leave it null).
Future<OidcTokens> loginWithOidc(
  String issuer,
  String clientId, {
  Future<void> Function(String url)? openUrl,
}) async {
  final popup = openUrl == null
      ? web.window.open('about:blank', 'parcello-oidc', 'width=480,height=680')
      : null;
  if (openUrl == null && popup == null) {
    throw 'popup blocked - allow popups for this site to sign in';
  }

  try {
    final endpoints = await discover(issuer);
    final verifier = randomUrlSafe(48);
    final state = randomUrlSafe(16);
    final origin = web.window.location.origin;
    final redirect = '$origin/oidc-callback.html';

    final authUrl =
        Uri.parse(endpoints.authorization).replace(queryParameters: {
      'client_id': clientId,
      'redirect_uri': redirect,
      'response_type': 'code',
      'scope': oidcScopes,
      'state': state,
      'code_challenge': pkceChallenge(verifier),
      'code_challenge_method': 'S256',
    });

    // Start listening before navigating - the redirect can otherwise land
    // before the listener is attached.
    final callback = _awaitCallback(origin);

    if (openUrl != null) {
      await openUrl(authUrl.toString());
    } else {
      popup!.location.href = authUrl.toString();
    }

    final result = await callback;
    if (result['state'] != state) throw 'login state mismatch';
    final code = result['code'] as String?;
    if (code == null) {
      throw 'login refused: ${result['error'] ?? 'no code returned'}';
    }

    final body = await exchangeToken(endpoints.token, {
      'grant_type': 'authorization_code',
      'code': code,
      'redirect_uri': redirect,
      'client_id': clientId,
      'code_verifier': verifier,
    });
    return OidcTokens.fromResponse(body);
  } finally {
    popup?.close();
  }
}

/// Listens for the `oidc-callback.html` page's `postMessage` and resolves
/// with its `code`/`state`/`error` payload. Removes its own listener once
/// resolved (success, timeout, or error) so repeated logins don't stack
/// listeners.
Future<Map<String, dynamic>> _awaitCallback(String origin) {
  final completer = Completer<Map<String, dynamic>>();
  late final JSFunction listener;

  void onMessage(web.Event event) {
    final message = event as web.MessageEvent;
    if (message.origin != origin) return;
    final data = message.data;
    if (data == null || !data.isA<JSString>()) return;
    final Map<String, dynamic> payload;
    try {
      payload = jsonDecode((data as JSString).toDart) as Map<String, dynamic>;
    } catch (_) {
      return;
    }
    if (payload['source'] != 'parcello-oidc-callback') return;
    if (!completer.isCompleted) completer.complete(payload);
  }

  listener = onMessage.toJS;
  web.window.addEventListener('message', listener);

  return completer.future
      .timeout(const Duration(minutes: 5),
          onTimeout: () => throw 'login timed out (no callback)')
      .whenComplete(() => web.window.removeEventListener('message', listener));
}
