/// Parcello Flutter client: desktop (Windows/Linux/macOS) and web from one
/// codebase. The server stays the only authority.
library;

import 'package:flutter/foundation.dart'
    show LicenseRegistry, LicenseEntryWithLineBreaks;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show rootBundle;

import 'l10n/app_localizations.dart';
import 'session.dart';
import 'tokens.dart';
import 'ui/connect_screen.dart';
import 'ui/game/game_screen.dart';
import 'ui/menu/menu_screen.dart';

void main() {
  _registerFontLicenses();
  runApp(ParcelloApp(session: GameSession()));
}

/// Make the bundled OFL font licences discoverable in-app (showLicensePage),
/// as the SIL Open Font License asks when a font is redistributed. The texts
/// ship as assets (see pubspec.yaml); this appends them to Flutter's registry
/// without replacing the framework's own entries.
void _registerFontLicenses() {
  LicenseRegistry.addLicense(() async* {
    for (final family in ['Inter', 'Fraunces', 'SourceSerif4']) {
      final text = await rootBundle.loadString('assets/fonts/$family-OFL.txt');
      yield LicenseEntryWithLineBreaks(['Parcello fonts', family], text);
    }
  });
}

class ParcelloApp extends StatelessWidget {
  final GameSession session;
  const ParcelloApp({super.key, required this.session});

  @override
  Widget build(BuildContext context) {
    // Only a language change rebuilds MaterialApp - deliberately NOT the
    // session's own notifier, which fires on every server update.
    return ValueListenableBuilder<String>(
      valueListenable: session.localeTag,
      builder: (context, tag, _) => _app(tag),
    );
  }

  /// `tag` empty = no override, so Flutter resolves the system locale.
  Widget _app(String tag) {
    return MaterialApp(
      locale: tag.isEmpty ? null : Locale(tag),
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: ThemeData(
        brightness: Brightness.dark,
        // Inter is the body/UI family (docs/visual-identity.md); Fraunces
        // (wordmark) and SourceSerif4 (tile labels) are applied at their
        // specific use sites. Bundled offline - assets/fonts/.
        fontFamily: 'Inter',
        scaffoldBackgroundColor: Pc.bg,
        colorScheme: ColorScheme.fromSeed(
          seedColor: Pc.gold,
          brightness: Brightness.dark,
        ).copyWith(surface: Pc.surface, error: Pc.oxblood),
        // Sharp corners everywhere: no pills, no soft blobs. Art direction, not
        // preference (`docs/visual-identity.md`).
        cardTheme: const CardThemeData(
            shape: RoundedRectangleBorder(borderRadius: Pc.radius)),
        filledButtonTheme: FilledButtonThemeData(
            style: FilledButton.styleFrom(
                shape: const RoundedRectangleBorder(borderRadius: Pc.radius))),
        outlinedButtonTheme: OutlinedButtonThemeData(
            style: OutlinedButton.styleFrom(
                shape: const RoundedRectangleBorder(borderRadius: Pc.radius))),
        dialogTheme: const DialogThemeData(
            shape: RoundedRectangleBorder(borderRadius: Pc.radius)),
      ),
      home: ListenableBuilder(
        listenable: session,
        builder: (context, _) {
          // This builder sits under MaterialApp's Localizations, so it is the
          // earliest place the (context-free) session can be handed its
          // AppLocalizations for the event log - refreshed every frame, set
          // before any server message is processed.
          session.l10n = AppLocalizations.of(context);
          if (session.joined) return GameScreen(s: session);
          if (session.connected) return MenuScreen(s: session);
          return ConnectScreen(s: session);
        },
      ),
    );
  }
}
