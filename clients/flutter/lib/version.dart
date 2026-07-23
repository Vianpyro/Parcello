/// The build's version token, shown in the app's version footer (menu and
/// connect screens).
///
/// Single source of truth: the workspace `version` in the root `Cargo.toml`.
/// It is injected at build time via `--dart-define=PARCELLO_VERSION=...` (by
/// every build path, from `tool/cargo_version.sh`) and read here as a
/// compile-time constant - the exact same transport as the git SHA below. This
/// is NOT a second source: pubspec.yaml carries only the fixed placeholder
/// Flutter requires, never a real version. There is no runtime platform lookup
/// and no `package_info`, so the label is identical on every platform (desktop
/// and web) and can never drift from the shipped build.
library;

/// Length of the abbreviated commit shown to players (git's short form).
const int _shaDisplayLen = 7;

/// The release version, injected from Cargo.toml at build time
/// (`--dart-define=PARCELLO_VERSION=...`). Empty for a plain `flutter run` that
/// did not pass it - the label then shows a `0.0.0-dev` marker.
const String appVersion = String.fromEnvironment('PARCELLO_VERSION');

/// The commit baked in at build time via
/// `--dart-define=PARCELLO_GIT_SHA=...` (CI passes the release commit; see the
/// Flutter workflows). Empty for a build that did not provide it - the label
/// then falls back to the bare version, never a placeholder.
const String appGitSha = String.fromEnvironment('PARCELLO_GIT_SHA');

/// Pure: the version token to display. `v1.2.3` normally, or
/// `v1.2.3 (abc1234)` when a commit was baked in. Accepts either a short or a
/// full-length sha (CI injects the 40-char `github.sha`) and normalises it to
/// [_shaDisplayLen].
String versionLabel(String version, {String gitSha = appGitSha}) {
  final base = 'v${version.trim()}';
  final sha = gitSha.trim();
  if (sha.isEmpty) return base;
  final short = sha.length > _shaDisplayLen ? sha.substring(0, _shaDisplayLen) : sha;
  return '$base ($short)';
}

/// The label to display, from the build-time constants. Falls back to a
/// `0.0.0-dev` marker when no version was injected (a local `flutter run`).
String appVersionLabel() =>
    versionLabel(appVersion.trim().isEmpty ? '0.0.0-dev' : appVersion);
