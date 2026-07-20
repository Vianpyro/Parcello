/// Step 1 of the client: pick a server and an identity.
library;

import 'dart:async';
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

/// The typed WebSocket URL's `/config.json` twin (ADR-0032): ws->http,
/// wss->https, same authority. Null when the URL doesn't parse yet.
Uri? configUrlFor(String wsUrl) {
  final uri = Uri.tryParse(wsUrl.trim());
  if (uri == null || uri.host.isEmpty) return null;
  final scheme = switch (uri.scheme) {
    'ws' => 'http',
    'wss' => 'https',
    _ => null,
  };
  if (scheme == null) return null;
  return Uri(
    scheme: scheme,
    host: uri.host,
    port: uri.hasPort ? uri.port : null,
    path: '/config.json',
  );
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _url = TextEditingController(text: defaultServerUrl());
  final _name = TextEditingController();
  final _token = TextEditingController();
  String? _signedInAs;

  /// Issuer default advertised by the target server (ADR-0032,
  /// `/config.json`). Null until fetched or when unset.
  String? _runtimeIssuer;

  /// Whether the target server accepts guests; null = unknown (old server,
  /// unreachable, or a cross-origin web fetch the browser blocked). Only a
  /// definitive `false` hides the guest path - unknown keeps it.
  bool? _guestAllowed;

  /// Whether `/config.json` answered at all: the newcomer's "is this
  /// address right?" signal. Null = still unknown (probe pending, or a
  /// web cross-origin failure that could just be CORS).
  bool? _reachable;

  Timer? _probeDebounce;
  int _probeGen = 0;

  @override
  void initState() {
    super.initState();
    _probeServer();
    // Re-probe as the player edits the address, debounced so a keystroke
    // burst costs one request.
    _url.addListener(() {
      _probeDebounce?.cancel();
      _probeDebounce = Timer(const Duration(milliseconds: 700), _probeServer);
    });
  }

  @override
  void dispose() {
    _probeDebounce?.cancel();
    super.dispose();
  }

  /// Best-effort probe of the *typed* server's `/config.json` (ADR-0032):
  /// a runtime-config fetch and an implicit liveness check in one. Never
  /// blocks connecting; on the web a cross-origin failure stays "unknown"
  /// (it may only be CORS), same-origin and desktop failures mean the
  /// server genuinely did not answer.
  Future<void> _probeServer() async {
    final target = configUrlFor(_url.text);
    final gen = ++_probeGen;
    if (target == null) {
      setState(() {
        _reachable = null;
        _guestAllowed = null;
        _runtimeIssuer = null;
      });
      return;
    }
    final definitiveFailure = !kIsWeb || target.origin == Uri.base.origin;
    try {
      final resp = await http
          .get(target)
          .timeout(const Duration(seconds: 3));
      if (!mounted || gen != _probeGen) return;
      if (resp.statusCode != 200) {
        // A failed probe must never keep the PREVIOUS server's answers:
        // stale guest_allowed=false would wrongly lock Connect against a
        // server whose policy is simply unknown.
        setState(() {
          _reachable = definitiveFailure ? false : null;
          _guestAllowed = null;
          _runtimeIssuer = null;
        });
        return;
      }
      final config = jsonDecode(resp.body) as Map<String, dynamic>;
      final issuer = config['default_issuer'];
      final guests = config['guest_allowed'];
      setState(() {
        _reachable = true;
        _runtimeIssuer =
            (issuer is String && issuer.isNotEmpty) ? issuer : null;
        _guestAllowed = guests is bool ? guests : null;
      });
    } catch (_) {
      if (!mounted || gen != _probeGen) return;
      setState(() {
        _reachable = definitiveFailure ? false : null;
        _guestAllowed = null;
        _runtimeIssuer = null;
      });
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
              padding: const EdgeInsets.all(Pc.s24),
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
                  const SizedBox(height: Pc.s2),
                  Text(t.connectSubtitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                  const SizedBox(height: Pc.s16),
                  TextField(
                    controller: _url,
                    decoration: InputDecoration(labelText: t.serverUrl),
                  ),
                  // Implicit liveness signal from the /config.json probe:
                  // tells a newcomer whether the pre-filled address needs
                  // changing before they ever hit Connect.
                  if (_reachable != null)
                    Padding(
                      padding: const EdgeInsets.only(top: Pc.s4),
                      child: Text(
                        _reachable == true
                            ? t.serverReachable
                            : t.serverUnreachable,
                        style: TextStyle(
                            fontSize: 11,
                            color: _reachable == true
                                ? Pc.textMuted
                                : Pc.oxblood),
                      ),
                    ),
                  TextField(
                    controller: _name,
                    maxLength: 24,
                    decoration: InputDecoration(labelText: t.displayName),
                  ),
                  // The server said guests are off: signing in is the only
                  // way in, so say so instead of letting a guest connect
                  // bounce off an auth error.
                  if (_guestAllowed == false && _signedInAs == null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: Pc.s6),
                      child: Text(t.serverRequiresAccount,
                          style: const TextStyle(
                              fontSize: 11, color: Pc.textMuted)),
                    ),
                  const SizedBox(height: Pc.s8),
                  wideButton(
                      _signedInAs == null
                          ? (_guestAllowed == false
                              ? t.signIn
                              : t.signInOptional)
                          : t.signedInAs(_signedInAs!),
                      _signIn,
                      primary: _guestAllowed == false && _signedInAs == null),
                  const SizedBox(height: 10),
                  wideButton(
                      t.connect,
                      // A guest connect is pointless against a server that
                      // said no guests: require the sign-in first.
                      _guestAllowed == false && _token.text.trim().isEmpty
                          ? null
                          : () {
                              if (_name.text.trim().isEmpty &&
                                  _token.text.trim().isEmpty) {
                                return;
                              }
                              s.connect(_url.text.trim(), _name.text.trim(),
                                  token: _token.text.trim());
                            }),
                  const SizedBox(height: Pc.s8),
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
