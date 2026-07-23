import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/version.dart';

void main() {
  test('versionLabel without a commit shows only the version', () {
    expect(versionLabel('0.22.1', gitSha: ''), 'v0.22.1');
  });

  test('versionLabel appends the short commit when one was baked in', () {
    expect(versionLabel('0.22.1', gitSha: '619a2f3'), 'v0.22.1 (619a2f3)');
  });

  test('versionLabel shortens a full 40-char sha to seven', () {
    // CI bakes ${{ github.sha }} (the full hash); the label normalises it.
    expect(
      versionLabel('0.22.1', gitSha: '619a2f3d4e5f60718293a4b5c6d7e8f901234567'),
      'v0.22.1 (619a2f3)',
    );
  });

  test('versionLabel trims whitespace/newlines around the injected sha', () {
    expect(versionLabel('0.22.1', gitSha: ' 619a2f3\n'), 'v0.22.1 (619a2f3)');
  });

  test('appVersionLabel falls back to a dev marker without a build-time define',
      () {
    // Unit tests run without --dart-define, so PARCELLO_VERSION is empty and
    // the label shows the dev marker rather than a bare or blank version.
    expect(appVersion, isEmpty);
    expect(appVersionLabel(), 'v0.0.0-dev');
  });
}
