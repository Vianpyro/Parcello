/// Step 1 of the client: pick a server and an identity.
library;

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../oidc.dart';
import '../session.dart';
import '../sfx.dart';
import '../tokens.dart';
import 'common.dart';
import 'language_button.dart';

/// The pre-filled server URL. A community server (ADR-0025) serves its own
/// Flutter Web build, so on web the WebSocket lives at the page's own origin -
/// derive it (http->ws, https->wss, keep any non-default port) so a hosted
/// player connects with zero typing and no personal domain is baked into the
/// repo. Desktop builds ship to LAN players, so they keep the loopback default.
String defaultServerUrl() {
  if (!kIsWeb) return 'ws://127.0.0.1:7878/ws';
  final base = Uri.base;
  final scheme = base.scheme == 'https' ? 'wss' : 'ws';
  final defaultPort = base.scheme == 'https' ? 443 : 80;
  final authority =
      base.port == defaultPort ? base.host : '${base.host}:${base.port}';
  return '$scheme://$authority/ws';
}

/// Pre-fill for the sign-in dialog's issuer field on a fresh browser (once a
/// player signs in, `savedIssuer` remembers their provider). Overridable at
/// build time so a community host bakes in its own provider without editing
/// source - `flutter build web --dart-define=PARCELLO_DEFAULT_ISSUER=https://auth.example.com`
/// (the Dockerfile forwards it as a build arg). The bare-scheme fallback keeps
/// the repo generic for anyone else self-hosting.
const _defaultIssuer =
    String.fromEnvironment('PARCELLO_DEFAULT_ISSUER', defaultValue: 'https://');

/// Step 1: connect to a server with an identity. The connection is kept open
/// so the menu (step 2) can create/join without reconnecting.
class ConnectScreen extends StatefulWidget {
  final GameSession s;
  const ConnectScreen({super.key, required this.s});

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _url = TextEditingController(text: defaultServerUrl());
  final _name = TextEditingController();
  final _token = TextEditingController();
  String? _signedInAs;

  /// OIDC login (ADR-0009): asks for the issuer URL, runs the browser
  /// PKCE flow, and drops the id_token into the token field.
  Future<void> _signIn() async {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    final issuer = TextEditingController(
        text: s.savedIssuer.isEmpty ? _defaultIssuer : s.savedIssuer);
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(t.signIn),
        content: TextField(
          controller: issuer,
          decoration: InputDecoration(
              labelText: t.identityProviderUrl,
              hintText: 'https://auth.example.com'),
        ),
        actions: [
          hoverSfx(TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: Text(t.cancel))),
          hoverSfx(FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: Text(t.openBrowser))),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    try {
      s.saveIssuer(issuer.text.trim());
      final token = await loginWithOidc(issuer.text.trim(), 'parcello');
      setState(() {
        _token.text = token;
        _signedInAs = jwtDisplayName(token) ?? t.account;
      });
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(t.signInFailed(e.toString()))));
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          child: Card(
            child: Container(
              width: 380,
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Align(
                      alignment: Alignment.centerRight,
                      child: LanguageButton(s: s)),
                  Text(t.appTitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                          fontSize: 30,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: 2),
                  Text(t.connectSubtitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                  const SizedBox(height: 16),
                  TextField(
                    controller: _url,
                    decoration: InputDecoration(labelText: t.serverUrl),
                  ),
                  TextField(
                    controller: _name,
                    maxLength: 24,
                    decoration: InputDecoration(labelText: t.displayName),
                  ),
                  const SizedBox(height: 8),
                  wideButton(
                      _signedInAs == null
                          ? t.signInOptional
                          : t.signedInAs(_signedInAs!),
                      _signIn,
                      primary: false),
                  const SizedBox(height: 10),
                  wideButton(t.connect, () {
                    if (_name.text.trim().isEmpty &&
                        _token.text.trim().isEmpty) {
                      return;
                    }
                    s.connect(_url.text.trim(), _name.text.trim(),
                        token: _token.text.trim());
                  }),
                  const SizedBox(height: 8),
                  Text(s.loginMessage,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
