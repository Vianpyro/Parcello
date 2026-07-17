/// The scrolling event feed in the board's centre.
library;

import 'package:flutter/material.dart';

import '../../tokens.dart';

class EventLog extends StatelessWidget {
  final List<String> log;
  const EventLog({super.key, required this.log});

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Pc.bg,
        border: Border.all(color: Pc.border),
        borderRadius: BorderRadius.circular(4),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: ListView.builder(
        reverse: true, // newest visible without scroll management
        itemCount: log.length,
        itemBuilder: (ctx, i) => Text(
          log[log.length - 1 - i],
          style: const TextStyle(fontSize: 11, color: Pc.textMuted),
        ),
      ),
    );
  }
}
