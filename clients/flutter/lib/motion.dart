/// The motion spec (`docs/motion-language.md`, ADR-0030): every duration,
/// curve, tier and budget in one place.
///
/// A duration literal anywhere else is a bug. This file exists because the
/// two halves of a beat used to derive their timing independently - the
/// director re-computed by hand what the pawn layer was doing, and its own
/// comment admitted it - so the two could silently drift. Both now read the
/// same constants.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): `Motion`, `Tier`, `Lane`,
/// `MotionProfile` are consumed app-wide (and their contract is bound to the
/// server's `ANIM_ACK_CAP` via ADR-0030). Values may be re-tuned; renaming or
/// removing a member, or changing a tier's meaning, needs a DDR - here it is
/// doubly so, because the budget is a cross-layer contract.
library;

import 'package:flutter/animation.dart';

/// Priority tiers. A tier is not a style: it is a contract about *who waits*.
enum Tier {
  /// P1 - the table stops. Irreversible, game-defining.
  arrest,

  /// P2 - a window is open and the clock is running.
  decide,

  /// P3 - money, property or points moved.
  consequence,

  /// P4 - context. Never enters the beat queue at all.
  ambient,
}

/// Whether a beat holds the plan open or rides alongside it.
enum Lane { exclusive, concurrent }

/// One knob, honoured everywhere. `instant` is not a degraded mode: it is the
/// same "I do not animate" path the CLI and bot seats already take under
/// ADR-0028, which is why the server needs no change to tolerate it.
enum MotionProfile {
  full,
  reduced,
  instant;

  /// Multiplier applied at exactly one place - the executor's wait.
  double get scale => switch (this) {
        MotionProfile.full => 1.0,
        MotionProfile.reduced => 0.5,
        MotionProfile.instant => 0.0,
      };

  /// Whether travelling primitives actually travel. Reduced motion fades them
  /// in at the target instead: the information (who paid whom) survives in the
  /// chit's text and colour, only the journey is dropped.
  bool get travels => this == MotionProfile.full;
}

abstract final class Motion {
  /// Hard ceiling on one Update's beats (ADR-0030), by the loudest beat in it.
  ///
  /// A plan over its budget is compressed, never played long: the server
  /// un-gates at `ANIM_ACK_CAP` = 10s and proceeds without us, and a client
  /// that outruns the cap is not slow - it is *behind the game*. The 2s margin
  /// under the cap absorbs frame-rate slop and a slow first paint.
  ///
  /// The budget is tiered because the tiers already say who is waiting and why:
  /// a bankruptcy or a win is the moment the whole table stops for, and it can
  /// afford eight seconds. A routine move cannot - it happens every twelve.
  static Duration budgetFor(Tier tier) => switch (tier) {
        Tier.arrest => const Duration(milliseconds: 8000),
        Tier.decide => const Duration(milliseconds: 6000),
        Tier.consequence || Tier.ambient => const Duration(milliseconds: 4000),
      };

  /// The ceiling any plan can claim - what the server's cap must clear.
  static const maxBudget = Duration(milliseconds: 8000);

  // -- movement ------------------------------------------------------------

  /// Per-tile hop rate. The count is the information (a 5 moved you 5 tiles -
  /// you should be able to count it), so a normal move hops rather than glides.
  static const hopPerTile = Duration(milliseconds: 260);

  /// A beat between the command landing and the pawn setting off, so the move
  /// reads as a decision followed by a consequence rather than one blur.
  static const hopWindUp = Duration(milliseconds: 160);

  /// A teleport (card, jail, backward) slides straight instead of hopping - it
  /// did not walk the intervening tiles and must not appear to.
  static const glide = Duration(milliseconds: 700);

  /// Beyond this many tiles a hop tapers instead of scaling linearly, so a
  /// forced long hop (a card wrapping through Go, which must be *seen*
  /// crossing Go or the salary chit has no visible cause) stays brisk.
  static const hopTaperFrom = 12;

  /// Total hop time for a `forward`-tile trip. Shared by the director (which
  /// pays for the beat) and the pawn layer (which runs it).
  static Duration hop(int forward) => Duration(
        milliseconds: forward <= hopTaperFrom
            ? (forward * hopPerTile.inMilliseconds).clamp(400, 3200)
            : (900 + forward * 60).clamp(900, 2400),
      );

  // -- beats ---------------------------------------------------------------

  /// A travelling chit reads in two beats, and the first one is the point.
  ///
  /// The chit appears at its source and **holds**, long enough to answer "how
  /// much, and from where", before anything moves. Only then does it travel.
  /// A chit that sets off immediately is a number you have to chase; a chit
  /// that states itself first is a number you read. (2026-07 playtest.)
  static const chitHold = Duration(milliseconds: 500);
  static const chitTravel = Duration(milliseconds: 500);

  /// What a chit beat costs: the hold plus the journey. Must equal
  /// `chitHold + chitTravel` - pinned by a test, because a mismatch would let
  /// the plan finish (and the ack fire) while money is still in the air.
  static const chit = Duration(milliseconds: 1000);

  /// Where the hold ends, as a fraction of the whole chit. The overlay drives
  /// its two phases from this, so the split lives in one place.
  static double get chitHoldFraction =>
      chitHold.inMilliseconds / chit.inMilliseconds;

  /// A card flips face-up and is held long enough to actually read.
  static const cardReveal = Duration(milliseconds: 1200);

  /// The movement card lifting out of the hand.
  static const cardPlay = Duration(milliseconds: 350);

  /// The jail hop: a straight slide, then the bars.
  static const jail = Duration(milliseconds: 800);

  /// P2 establish: recede the board, lift the subject.
  static const establish = Duration(milliseconds: 700);

  /// Sealed bids flipping face-up, plus the hold that makes them comparable.
  static const bidReveal = Duration(milliseconds: 1100);

  /// A tile's band taking a new owner's colour.
  static const bandSweep = Duration(milliseconds: 400);

  /// Stagger between coalesced siblings (a portfolio changing hands at once).
  static const stagger = Duration(milliseconds: 40);

  /// A tile or button refusing an action. The only lateral shake in the game:
  /// "no" is a physical gesture, and it belongs on the thing that said it.
  static const refuse = Duration(milliseconds: 300);

  /// P1. The payload is the hold, not the motion.
  static const arrest = Duration(milliseconds: 1600);
  static const arrestWin = Duration(milliseconds: 2000);

  /// How long a P1 must be watched before it can be skipped.
  static const arrestFloor = Duration(milliseconds: 400);

  /// A banner (spotlight, market event) held long enough to read.
  static const banner = Duration(milliseconds: 900);

  /// P4 implicit transitions. Barely noticed, by design.
  static const ambient = Duration(milliseconds: 120);

  /// Re-orientation after a reconnect: "here is you, here is now". Never a
  /// catch-up replay - a reconnecting client is already late.
  static const reorient = Duration(milliseconds: 900);

  // -- curves --------------------------------------------------------------
  //
  // No springs, no elastic, no bounce, anywhere. Bounce reads as toy; Art Deco
  // is arrival and symmetry - motion resolves and *stops*. One bouncy element
  // would undo the whole register.

  /// P3: decisive arrival, no overshoot.
  static const arrive = Curves.easeOutCubic;

  /// P2: deliberate.
  static const deliberate = Curves.easeInOutCubic;

  /// P1: inevitable.
  static const inevitable = Curves.easeOutQuint;

  /// Something was done *to* you (takeover, boost trap, bankruptcy): snaps in
  /// without a ramp and lingers. The only asymmetric curve in the game, and
  /// its asymmetry is the message.
  static const threat = Curves.easeInCubic;

  /// P4.
  static const ambientCurve = Curves.easeOut;
}
