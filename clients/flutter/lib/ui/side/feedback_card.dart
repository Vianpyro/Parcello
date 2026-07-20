/// The post-game survey: a side card, never a modal - it must not block.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';

class FeedbackCard extends StatefulWidget {
  final GameSession s;
  const FeedbackCard({super.key, required this.s});

  @override
  State<FeedbackCard> createState() => FeedbackCardState();
}

class FeedbackCardState extends State<FeedbackCard> {
  int _rating = 0;
  final _comment = TextEditingController();

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(Pc.s12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(
              child: Text(t.feedbackTitle,
                  style: const TextStyle(
                      fontSize: 12,
                      color: Pc.textMuted,
                      letterSpacing: 1)),
            ),
            hoverSfx(IconButton(
              icon: const Icon(Icons.close, size: 16),
              onPressed: s.dismissFeedback,
              tooltip: t.feedbackDismiss,
            )),
          ]),
          Row(children: [
            for (var star = 1; star <= 5; star++)
              hoverSfx(IconButton(
                icon: Icon(
                  star <= _rating ? Icons.star : Icons.star_border,
                  color: Pc.gold,
                ),
                onPressed: () => setState(() => _rating = star),
              )),
          ]),
          TextField(
            controller: _comment,
            maxLength: 500,
            decoration: InputDecoration(
                labelText: t.feedbackCommentHint, counterText: ''),
          ),
          const SizedBox(height: Pc.s6),
          hoverSfx(FilledButton(
            onPressed: _rating == 0
                ? null
                : () => s.sendFeedback(_rating, _comment.text),
            child: Text(t.feedbackSend),
          )),
        ]),
      ),
    );
  }
}
