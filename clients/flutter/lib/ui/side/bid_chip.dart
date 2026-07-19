/// One seat's sealed bid, revealed (ADR-0018).
library;

import 'package:flutter/material.dart';

import '../../motion.dart';
import '../../reveal.dart';
import '../../tokens.dart';

class BidChip extends StatelessWidget {
  final int bid;
  final bool won;
  const BidChip({super.key, required this.bid, required this.won});

  @override
  Widget build(BuildContext context) {
    return Reveal(
      duration: Motion.bidReveal,
      curve: Motion.arrive,
      builder: (context, t, child) => Transform(
        alignment: Alignment.center,
        // A card turning over, not a number appearing.
        transform: Matrix4.identity()..rotateX((1 - t) * 1.4),
        child: Opacity(opacity: t, child: child),
      ),
      child: Container(
        margin: const EdgeInsets.only(right: 6),
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
        decoration: BoxDecoration(
          color: won ? Pc.gold : Pc.parchment,
          borderRadius: Pc.radius,
          border: Border.all(color: won ? Pc.goldDark : Pc.border),
        ),
        child: Text(
          // Zero is an abstention, and it reads as one.
          bid == 0 ? '--' : '\$$bid',
          style: const TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w800,
            color: Pc.parchmentInk,
            fontFeatures: [FontFeature.tabularFigures()],
          ),
        ),
      ),
    );
  }
}

/// Lobby settings panel (ADR-0015): the host (seat 0) edits timers and rules
/// for this game; everyone else sees them read-only. Collapsed by default so
/// the lobby stays tidy. Settings freeze once the game starts.
