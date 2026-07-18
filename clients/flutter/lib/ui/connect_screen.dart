/// Step 1 of the client: pick a server and an identity.
library;

import 'dart:convert';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;

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

/// Compile-time fallback for the sign-in dialog's issuer field. The server's
/// runtime `/config.json` (ADR-0032) overrides it when set, and a player's
/// remembered provider (`savedIssuer`) overrides both. Baked at build time so
/// a source-built or desktop client can still carry a default -
/// `--dart-define=PARCELLO_DEFAULT_ISSUER=https://auth.example.com` (the
/// Dockerfile forwards it as a build arg). The bare-scheme default keeps the
/// repo generic for anyone else self-hosting.
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

  /// Issuer default advertised by the server that served this web build
  /// (ADR-0032, `/config.json`). Null until fetched, or when unset/native.
  String? _runtimeIssuer;

  @override
  void initState() {
    super.initState();
    // Only the web build is served by a Parcello server it can query for its
    // configured issuer; a desktop client connects to arbitrary servers, so
    // it keeps the compile-time default.
    if (kIsWeb) _loadRuntimeConfig();
  }

  /// Best-effort: a missing/broken `/config.json` just leaves the sign-in
  /// field on its compile-time default. Never blocks or surfaces an error.
  Future<void> _loadRuntimeConfig() async {
    try {
      final resp = await http.get(Uri.base.resolve('/config.json'));
      if (resp.statusCode != 200) return;
      final issuer =
          (jsonDecode(resp.body) as Map<String, dynamic>)['default_issuer'];
      if (issuer is String && issuer.isNotEmpty && mounted) {
        setState(() => _runtimeIssuer = issuer);
      }
    } catch (_) {
      // ignored on purpose: the default issuer is a convenience, not required.
    }
  }

  /// OIDC login (ADR-0009): asks for the issuer URL, runs the browser
  /// PKCE flow, and drops the id_token into the token field.
  Future<void> _signIn() async {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    final issuer = TextEditingController(
        text: s.savedIssuer.isNotEmpty
            ? s.savedIssuer
            : (_runtimeIssuer ?? _defaultIssuer));
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
        final handle = jwtDisplayName(token);
        _signedInAs = handle ?? t.account;
        // Seed the display-name field with the account's name as the default
        // in-game handle (ADR-0033); the player can still edit it before
        // connecting. Never clobber a name they already typed.
        if (handle != null && _name.text.trim().isEmpty) _name.text = handle;
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
