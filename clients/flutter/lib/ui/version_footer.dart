/// The build's version (and short commit when baked in) as a muted footer line.
///
/// Shared by the menu and the connect screen so the version is visible before
/// AND after sign-in from one implementation (the app has no settings/about
/// surface). The version is a compile-time constant (see lib/version.dart), so
/// this is a plain stateless label - no async load, no platform channel.
library;

import 'package:flutter/material.dart';

import '../tokens.dart';
import '../typography.dart';
import '../version.dart';

class VersionFooter extends StatelessWidget {
  const VersionFooter({super.key});

  @override
  Widget build(BuildContext context) {
    return Text(appVersionLabel(),
        style: PcText.label.copyWith(color: Pc.textMuted));
  }
}
