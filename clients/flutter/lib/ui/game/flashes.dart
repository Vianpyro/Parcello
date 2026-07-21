/// The two things the board's centre announces: the played movement card,
/// and the one banner every reveal shares (motion-language.md).
library;

import 'dart:async';

import 'package:flutter/material.dart';

import '../../motion.dart';
import '../../stage.dart';
import '../../tokens.dart';

class CardFlash extends StatefulWidget {
  final int seq, value;
  const CardFlash({super.key, required this.seq, required this.value});

  @override
  State<CardFlash> createState() => CardFlashState();
}

class CardFlashState extends State<CardFlash> {
  bool _visible = false;
  Timer? _timer;

  @override
  void didUpdateWidget(CardFlash oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.seq != oldWidget.seq && widget.seq > 0) {
      setState(() => _visible = true);
      _timer?.cancel();
      _timer = Timer(const Duration(milliseconds: 1500), () {
        if (mounted) setState(() => _visible = false);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: AnimatedOpacity(
        opacity: _visible ? 1 : 0,
        duration: Motion.cardPlay,
        curve: Motion.arrive,
        child: Container(
          width: 66,
          height: 66,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: Pc.parchment,
            borderRadius: Pc.radius,
            border: Border.all(color: Pc.goldDark, width: 1.5),
            boxShadow: Pc.hairShadow,
          ),
          child: Text(
            '${widget.value}',
            style: const TextStyle(
                fontSize: 32,
                fontWeight: FontWeight.bold,
                color: Pc.parchmentInk,
                fontFeatures: [FontFeature.tabularFigures()]),
          ),
        ),
      ),
    );
  }
}

/// A one-shot banner over the board: a drawn card, a spotlight, a market event.
/// One shape, one place, every time - a player should never have to work out
/// *where* the game is going to tell them something.
class BannerFlash extends StatefulWidget {
  final int seq;
  final String text;
  final BannerKind kind;
  const BannerFlash({
    super.key,
    required this.seq,
    required this.text,
    required this.kind,
  });

  @override
  State<BannerFlash> createState() => BannerFlashState();
}

class BannerFlashState extends State<BannerFlash> {
  bool _visible = false;
  Timer? _timer;

  @override
  void didUpdateWidget(BannerFlash oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.seq != oldWidget.seq && widget.seq > 0) {
      setState(() => _visible = true);
      _timer?.cancel();
      // Held for as long as the beat the director paid for - the two must agree,
      // or the banner outlives the pause that exists to let it be read.
      final hold = widget.kind == BannerKind.card
          ? Motion.cardReveal
          : Motion.banner;
      _timer = Timer(hold, () {
        if (mounted) setState(() => _visible = false);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Paper for a card read; a dark plate for a world event. The register tells
    // you which kind of thing just happened before you read a word of it.
    final paper = widget.kind == BannerKind.card;
    return IgnorePointer(
      child: AnimatedOpacity(
        opacity: _visible ? 1 : 0,
        duration: Motion.ambient,
        child: Container(
          constraints: const BoxConstraints(maxWidth: 320),
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: Pc.s12),
          decoration: BoxDecoration(
            color: paper ? Pc.parchment : Pc.surface,
            borderRadius: Pc.radius,
            border: Border.all(color: Pc.goldDark, width: 1.5),
            boxShadow: Pc.hairShadow,
          ),
          child: Text(
            widget.text,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w600,
              color: paper ? Pc.parchmentInk : Pc.text,
            ),
          ),
        ),
      ),
    );
  }
}

/// Caps a numeric text field at `max`, clamping down any edit that would
/// exceed it (used for the sealed-bid amount, bounded by the seat's cash).
/// Empty input passes through so the field can be cleared and retyped.
