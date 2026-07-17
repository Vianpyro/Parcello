/// The UI language picker. Lives at `ui/`'s root rather than under `menu/`
/// because both the connect screen and the menu carry it - someone whose OS is
/// in another language needs it before they ever reach the menu.
library;

import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../session.dart';
import '../sfx.dart';
import '../tokens.dart';

/// Language names are endonyms - a language is always named in its own
/// language, never translated - so they are data here rather than ARB strings.
/// Keyed by the codes in `AppLocalizations.supportedLocales`; a locale added
/// without an entry falls back to its code rather than disappearing.
const _languageNames = {'en': 'English', 'fr': 'Français'};

/// Picks the UI language: system default, or a forced locale.
class LanguageButton extends StatelessWidget {
  final GameSession s;
  const LanguageButton({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return hoverSfx(PopupMenuButton<String>(
      icon: const Icon(Icons.language, size: 18, color: Pc.textMuted),
      tooltip: t.language,
      initialValue: s.localeTag.value,
      onSelected: s.setLocaleTag,
      itemBuilder: (_) => [
        PopupMenuItem(value: '', child: Text(t.languageSystem)),
        for (final l in AppLocalizations.supportedLocales)
          PopupMenuItem(
            value: l.languageCode,
            child: Text(_languageNames[l.languageCode] ?? l.languageCode),
          ),
      ],
    ));
  }
}
