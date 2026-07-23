/// The credential's lifecycle (ADR-0037): one place that knows what the
/// current identity token is, when it dies, and how to renew it.
///
/// Before this existed, `loginWithOidc`'s id_token was held as a bare
/// String for the whole run of the app - and the refresh token that came
/// back in the same response was never even parsed. The server verifies
/// `exp` on every auth-carrying message (`ws.rs::authenticate`), so at the
/// issuer's configured lifetime that credential silently became worthless
/// and every route back into a room - join, rejoin after a dropped socket,
/// spectate - failed permanently, while the means to renew it sat unread.
/// Renewal is not a nicety here; it is what makes a session outlive a
/// token.
///
/// Guests have no manager: `AuthManager.guest` carries a name and nothing
/// to renew, so the whole file is a no-op for them.
library;

import 'dart:async';

import 'oidc.dart';

/// How long before `exp` the proactive timer renews. Wide enough that a
/// slow issuer or a retry still lands ahead of the deadline, small enough
/// that a token is used for nearly its whole life - one refresh per
/// lifetime, not a poll.
const _renewMargin = Duration(seconds: 120);

/// Never arm the timer closer than this: a token that is already nearly
/// dead when we receive it gets one prompt attempt rather than a refresh
/// storm.
const _minRenewDelay = Duration(seconds: 30);

/// Lazy guard used at the point of use. Slightly tighter than
/// [_renewMargin] so the timer normally wins the race and the lazy path
/// only fires for the case timers cannot cover: a suspended machine that
/// wakes up past its renewal point.
const _staleWithin = Duration(seconds: 60);

/// Owns the identity credential for one connected session.
///
/// The refresh token never leaves this object: not to `reconnect.json`,
/// not to `localStorage`, not to the log. It dies with [clear].
class AuthManager {
  /// Public in-game handle (ADR-0033), or the guest name when [tokens] is
  /// null. Not identity - the server takes identity from the token's `sub`.
  final String displayName;

  /// Issuer and client the grant came from; needed to redeem a refresh.
  /// Null for a guest, or for a token pasted in by hand (the CLI/web
  /// escape hatch) - such a session simply cannot renew.
  final String? issuer;
  final String? clientId;

  OidcTokens? _tokens;

  /// Cached token endpoint, so a renewal costs one request rather than a
  /// discovery round trip plus one.
  String? _tokenEndpoint;

  Timer? _renewTimer;

  /// In-flight refresh, so concurrent callers share one round trip
  /// (a reconnect can easily ask twice in the same tick).
  Future<String?>? _inFlight;

  /// Set once the issuer has refused the grant itself (`invalid_grant`) or
  /// there was never a refresh token to begin with and the token has since
  /// expired. The UI uses it to say "sign in again" instead of retrying.
  bool signInRequired = false;

  /// Set when a refresh came back healthy but WITHOUT an ID token - an
  /// issuer that will not reissue what Parcello authenticates with
  /// (ADR-0009 amendment 2). Sessions on such a deployment end at `exp`
  /// no matter what this class does; the flag exists so that shows up as
  /// a diagnosable configuration fact rather than an endless retry.
  bool cannotRenew = false;

  /// Called when the credential changes state, so the UI can react
  /// (currently: surface [signInRequired]). Optional.
  void Function()? onChanged;

  AuthManager({
    required this.displayName,
    OidcTokens? tokens,
    this.issuer,
    this.clientId,
  }) : _tokens = tokens {
    if (tokens != null) _armRenewal(tokens);
  }

  /// A guest identity: a name, no token, nothing to renew.
  AuthManager.guest(String name) : this(displayName: name);

  /// True when this session authenticates with a token rather than a name.
  bool get hasToken => _tokens != null;

  /// The current id_token without touching the network - for UI that only
  /// wants to read a claim. Use [freshIdToken] for anything sent to the
  /// server.
  String? get currentIdToken => _tokens?.idToken;

  /// Whether this session is able to renew itself at all. False for
  /// guests, for hand-pasted tokens, and for an issuer that granted no
  /// refresh token - those sessions still work, they just end at `exp`.
  bool get canRenew =>
      _tokens?.refreshToken != null && issuer != null && clientId != null;

  /// The token to put in an `auth` payload, renewed first if it is at or
  /// near its expiry. Null for a guest.
  ///
  /// This is the lazy half of the policy. The timer below normally renews
  /// well before anyone asks - but timers do not fire while a laptop is
  /// suspended, and "the machine woke up past `exp`" is precisely the case
  /// that used to end a session. So every use checks.
  Future<String?> freshIdToken() {
    final tokens = _tokens;
    if (tokens == null) return Future.value(null);
    if (!_isStale(tokens)) return Future.value(tokens.idToken);
    return _renew();
  }

  /// True once the token is inside [_staleWithin] of expiry (or already
  /// past it). A token with no `exp` cannot be reasoned about - treat it
  /// as fresh and let the server be the judge.
  bool _isStale(OidcTokens tokens) {
    final expiry = tokens.expiresAt;
    if (expiry == null) return false;
    return DateTime.now().isAfter(expiry.subtract(_staleWithin));
  }

  /// Single-flight refresh. Returns the new id_token, or the old one when
  /// renewal is impossible or failed transiently - sending a token the
  /// server may still accept beats sending nothing, and a hard refusal has
  /// already flipped [signInRequired].
  Future<String?> _renew() {
    return _inFlight ??= _refresh().whenComplete(() => _inFlight = null);
  }

  Future<String?> _refresh() async {
    final tokens = _tokens;
    final refresh = tokens?.refreshToken;
    if (tokens == null) return null;
    if (refresh == null || issuer == null || clientId == null) {
      // Nothing to renew with. Only a token that is actually past its
      // expiry is a dead end worth telling the player about.
      _requireSignIn(expired: _isExpired(tokens));
      return tokens.idToken;
    }
    try {
      final endpoint = _tokenEndpoint ??= (await discover(issuer!)).token;
      final renewed = await refreshTokens(endpoint, clientId!, refresh);
      _tokens = renewed;
      signInRequired = false;
      _armRenewal(renewed);
      onChanged?.call();
      return renewed.idToken;
    } on OidcRefreshRejected {
      // The issuer has disowned this grant; retrying cannot help.
      _requireSignIn(expired: true);
      return tokens.idToken;
    } on OidcNoIdToken {
      // The refresh succeeded but brought no ID token back, which OIDC
      // Core section 12.2 allows. Parcello authenticates with the ID
      // token (ADR-0009 amendment 2), so this issuer cannot renew a
      // session - and no amount of retrying will change that. Stop, and
      // record it so the failure is diagnosable rather than mysterious.
      cannotRenew = true;
      _renewTimer?.cancel();
      _renewTimer = null;
      _requireSignIn(expired: true);
      return tokens.idToken;
    } catch (_) {
      // Transient (network, issuer 5xx). Keep what we have and let the
      // next use - or the retry armed below - try again.
      _armRetry();
      _requireSignIn(expired: _isExpired(tokens));
      return tokens.idToken;
    }
  }

  bool _isExpired(OidcTokens tokens) {
    final expiry = tokens.expiresAt;
    return expiry != null && DateTime.now().isAfter(expiry);
  }

  void _requireSignIn({required bool expired}) {
    if (signInRequired == expired) return;
    signInRequired = expired;
    onChanged?.call();
  }

  /// Renew once per token lifetime, [_renewMargin] ahead of `exp`. A token
  /// with no expiry gets no timer - there is no deadline to beat.
  void _armRenewal(OidcTokens tokens) {
    _renewTimer?.cancel();
    _renewTimer = null;
    final expiry = tokens.expiresAt;
    if (expiry == null || tokens.refreshToken == null) return;
    final until = expiry.difference(DateTime.now()) - _renewMargin;
    final delay = until < _minRenewDelay ? _minRenewDelay : until;
    _renewTimer = Timer(delay, () => unawaited(_renew()));
  }

  /// After a transient failure, try again before the token is gone rather
  /// than waiting for someone to need it.
  void _armRetry() {
    _renewTimer?.cancel();
    _renewTimer = Timer(_minRenewDelay, () => unawaited(_renew()));
  }

  /// Drops the credential. Called when the player disconnects from the
  /// server: the refresh token must not outlive the session that needed it.
  void clear() {
    _renewTimer?.cancel();
    _renewTimer = null;
    _tokens = null;
    _inFlight = null;
    _tokenEndpoint = null;
    signInRequired = false;
    cannotRenew = false;
  }
}
