/// Step 1 of the client: pick a server and an identity.
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;

import '../l10n/app_localizations.dart';
import '../design/components/pc_button.dart';
import '../design/components/pc_card.dart';
import '../design/components/pc_dialog.dart';
import '../design/components/pc_textfield.dart';
import '../oidc.dart';
import '../session.dart';
import '../tokens.dart';
import '../typography.dart';
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
      builder: (ctx) => PcDialog(
        title: t.signIn,
        content: PcTextField(
          controller: issuer,
          label: t.identityProviderUrl,
          hint: 'https://auth.example.com',
        ),
        cancelLabel: t.cancel,
        primaryLabel: t.openBrowser,
        onPrimary: () => Navigator.pop(ctx, true),
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
    // Guests-off + not signed in: sign-in is the ONLY way in, so it becomes
    // the primary action and Connect is disabled with a reason.
    final guestsOff = _guestAllowed == false && _signedInAs == null;
    final connectDisabled =
        _guestAllowed == false && _token.text.trim().isEmpty;
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          // PcCard is FLAT (no Material shadow) - the reference screen now
          // demonstrates the flat register. Width is the screen's concern,
          // not the card's, so it is constrained from outside.
          child: SizedBox(
            width: 380,
            child: PcCard(
              padding: const EdgeInsets.all(Pc.s24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Align(
                      alignment: Alignment.centerRight,
                      child: LanguageButton(s: s)),
                  // The wordmark: Fraunces, the brand voice (DDR-0018 /
                  // TYPOGRAPHY.md - Fraunces is reserved for the brand, Inter
                  // is the default UI face), at hero size.
                  Text(t.appTitle,
                      textAlign: TextAlign.center,
                      style: PcText.wordmark.copyWith(fontSize: 30)),
                  const SizedBox(height: Pc.s2),
                  // FRICTION (typography/API): no PcText role expresses "muted
                  // at the ambient size" - roles carry a size (DDR-0018), and
                  // the theme sets no default text colour. Left bespoke; see
                  // DESIGN/DESIGN_FEEDBACK.md.
                  Text(t.connectSubtitle,
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: Pc.textMuted)),
                  const SizedBox(height: Pc.s16),
                  PcTextField(controller: _url, label: t.serverUrl),
                  // Implicit liveness signal from the /config.json probe.
                  if (_reachable != null)
                    Padding(
                      padding: const EdgeInsets.only(top: Pc.s4),
                      child: Text(
                        _reachable == true
                            ? t.serverReachable
                            : t.serverUnreachable,
                        style: PcText.caption.copyWith(
                            color: _reachable == true
                                ? Pc.textMuted
                                : Pc.oxblood),
                      ),
                    ),
                  PcTextField(
                      controller: _name, label: t.displayName, maxLength: 24),
                  const SizedBox(height: Pc.s8),
                  PcButton(
                    _signedInAs == null
                        ? (guestsOff ? t.signIn : t.signInOptional)
                        : t.signedInAs(_signedInAs!),
                    onPressed: _signIn,
                    variant: guestsOff
                        ? PcButtonVariant.primary
                        : PcButtonVariant.secondary,
                  ),
                  const SizedBox(height: 10),
                  PcButton(
                    t.connect,
                    onPressed: connectDisabled
                        ? null
                        : () {
                            if (_name.text.trim().isEmpty &&
                                _token.text.trim().isEmpty) {
                              return;
                            }
                            s.connect(_url.text.trim(), _name.text.trim(),
                                token: _token.text.trim());
                          },
                    // The disabled reason now lives ON the button (the DS
                    // pattern), replacing the former separate caption above -
                    // it now sits under the control it explains.
                    disabledReason:
                        connectDisabled ? t.serverRequiresAccount : null,
                  ),
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
