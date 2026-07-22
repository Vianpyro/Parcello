/// The build's version token, shown in the main menu footer.
///
/// The version string has a single source of truth - `pubspec.yaml` (kept in
/// step with the Cargo workspace version, the release contract in
/// `.github/workflows/release.yml`) - read at runtime via `package_info_plus`.
/// Nothing here hardcodes it, so the label can never drift from the shipped
/// build.
library;

import 'package:package_info_plus/package_info_plus.dart';

/// Length of the abbreviated commit shown to players (git's conventional
/// short form).
const int _shaDisplayLen = 7;

/// The commit baked in at build time via
/// `--dart-define=PARCELLO_GIT_SHA=...` (CI passes the release commit; see the
/// Flutter workflows). Empty for a plain `flutter run`/`flutter build` that did
/// not provide it - the label then falls back to the bare version, never a
/// placeholder.
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

/// Reads the app version from the platform bundle (derived from `pubspec.yaml`)
/// and formats it with the optional baked-in commit.
Future<String> loadVersionLabel() async {
  final info = await PackageInfo.fromPlatform();
  return versionLabel(info.version);
}
