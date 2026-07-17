/// The static rules reference, reached from the menu.
library;

import 'package:flutter/material.dart';
import '../../back_on_escape.dart';
import '../../l10n/app_localizations.dart';
import '../../tokens.dart';

/// Static rules reference reached from the menu. Deliberately concise: the
/// Business-Tour differences a new player needs, not the full engine spec.
class RulesScreen extends StatelessWidget {
  const RulesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    final sections = <(String, String)>[
      (t.rulesGoalTitle, t.rulesGoalBody),
      (t.rulesMoveTitle, t.rulesMoveBody),
      (t.rulesAuctionTitle, t.rulesAuctionBody),
      (t.rulesBuildTitle, t.rulesBuildBody),
      (t.rulesJailTitle, t.rulesJailBody),
      (t.rulesWinTitle, t.rulesWinBody),
    ];
    // Escape (controller B via Steam Input) pops back to the menu.
    return BackOnEscape(
      child: Scaffold(
        appBar: AppBar(title: Text(t.rulesTitle), backgroundColor: Pc.surface2),
        body: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 640),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(t.rulesTagline,
                      style: const TextStyle(
                          fontSize: 16,
                          fontStyle: FontStyle.italic,
                          color: Pc.gold)),
                  const SizedBox(height: 20),
                  for (final (title, body) in sections) ...[
                    Text(title,
                        style: const TextStyle(
                            fontFamily: 'Fraunces',
                            fontSize: 20,
                            fontWeight: FontWeight.w700,
                            color: Pc.text)),
                    const SizedBox(height: 6),
                    Text(body,
                        style: const TextStyle(
                            fontSize: 15, height: 1.4, color: Pc.text)),
                    const SizedBox(height: 20),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
