/// Small helpers shared by more than one screen. Everything here is used from
/// at least two of `ui/`'s subtrees - a widget used by exactly one screen
/// belongs next to that screen instead.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../sfx.dart';

/// Copies a room code and confirms with a brief snackbar.
void copyCode(BuildContext context, String code) {
  Clipboard.setData(ClipboardData(text: code));
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(
    content: Text(AppLocalizations.of(context).roomCodeCopied(code)),
    duration: const Duration(seconds: 1),
  ));
}

/// A tall, full-width button so every screen ports to touch with minimal
/// change. Primary = filled, secondary = outlined.
Widget wideButton(String label, VoidCallback? onPressed, {bool primary = true}) {
  final style = ButtonStyle(
    minimumSize: WidgetStateProperty.all(const Size.fromHeight(52)),
    textStyle: WidgetStateProperty.all(
        const TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
  );
  return hoverSfx(primary
      ? FilledButton(onPressed: onPressed, style: style, child: Text(label))
      : OutlinedButton(onPressed: onPressed, style: style, child: Text(label)));
}
