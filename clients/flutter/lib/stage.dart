/// Transient visual state: what the board is currently *showing*, as opposed
/// to what the server says is *true* (that is `GameSession.view`).
///
/// This is a separate notifier on purpose. Animation frames repaint the board
/// forty times a second; they must not repaint the action panel, because that
/// panel holds text fields a player is typing into. The old design shared one
/// notifier and paid for it with two guards (`_bidInitTile`, `_bribeSeeded`)
/// that existed solely to stop an animation frame from wiping a half-typed
/// bid. Splitting the notifier removes the class of bug rather than the two
/// instances of it.
library;

import 'package:flutter/material.dart';

import 'motion.dart';

/// Where a travelling primitive starts or ends. Chits fly between the board
/// and the side panel, so an anchor has to resolve across widget subtrees.
sealed class Anchor {
  const Anchor();
}

final class TileAnchor extends Anchor {
  final int tile;
  const TileAnchor(this.tile);
}

final class SeatAnchor extends Anchor {
  final int seat;
  const SeatAnchor(this.seat);
}

/// Resolves anchors to screen coordinates. The board installs a resolver for
/// tiles (it alone knows the ring geometry); seat markers hand over a key each.
class AnchorRegistry {
  /// Global-coordinate centre of a board tile. Installed by the board.
  Offset? Function(int tile)? tiles;

  final Map<int, GlobalKey> _seats = {};

  GlobalKey seatKey(int seat) => _seats.putIfAbsent(seat, GlobalKey.new);

  /// Null when the anchor is not laid out yet (first frame, or a seat scrolled
  /// out of view). Callers fall back to a non-travelling presentation.
  Offset? resolve(Anchor a) => switch (a) {
        TileAnchor(:final tile) => tiles?.call(tile),
        SeatAnchor(:final seat) => _centerOf(_seats[seat]),
      };

  static Offset? _centerOf(GlobalKey? key) {
    final box = key?.currentContext?.findRenderObject() as RenderBox?;
    if (box == null || !box.hasSize) return null;
    return box.localToGlobal(box.size.center(Offset.zero));
  }
}

/// What a travelling primitive *is*. Shape encodes category
/// (`motion-language.md` 4.3): money is a parchment chit, victory points are a
/// gold chevron - and gold that moves always means victory points, nothing else.
///
/// Money is typed *per observer*, not per event: one rent payment is a loss to
/// the payer, a gain to the owner, and neither to the table. The same chit,
/// read from three seats - which is what makes "who paid whom" free, and what
/// keeps an attack on you from ever being ambient.
enum ChitKind { gain, loss, neutral, victoryPoints }

/// One value in flight from a source to a target. A rent payment is ONE of
/// these, not two floaters: "who paid whom" is then the shape of the motion
/// rather than something the player has to infer.
class Chit {
  final int id;
  final Anchor from;
  final Anchor to;
  final String text;
  final ChitKind kind;

  /// Reduced motion drops the journey, not the information: the chit fades in
  /// at its target with the same text and colour.
  final bool travels;

  /// The value grew on the way (a boost trap sprang over it) - the causal link
  /// between "the trap fired" and "that number is huge".
  final bool amplified;

  const Chit({
    required this.id,
    required this.from,
    required this.to,
    required this.text,
    required this.kind,
    this.travels = true,
    this.amplified = false,
  });
}

/// The sealed bids, face-up (ADR-0018). The most information-dense moment in
/// Parcello, and the one the old client never rendered at all.
class BidReveal {
  final int tile;
  final List<int> bids; // one per seat, 0 = abstained
  final int? winner;
  final int amount;

  /// The contested-win discount applied - the chit shrinks on its way to the
  /// tile, because the discount is a thing that happens to the money in flight.
  final bool discounted;

  const BidReveal({
    required this.tile,
    required this.bids,
    required this.winner,
    required this.amount,
    this.discounted = false,
  });
}

/// P1. The board recedes and the table stops.
class Arrest {
  final String title;
  final String? detail;

  /// Highlighted through the recede, at full opacity.
  final int? seat;
  const Arrest({required this.title, this.detail, this.seat});
}

enum BannerKind { card, spotlight, market }

class StageState extends ChangeNotifier {
  final AnchorRegistry anchors = AnchorRegistry();

  /// Reduced/instant honour the platform accessibility flag by default; the
  /// player can override. Read at exactly one place - the executor's wait -
  /// plus [Chit.travels].
  MotionProfile profile = MotionProfile.full;

  // -- pawns ---------------------------------------------------------------

  /// Director-driven positions, advanced beat by beat so a multi-hop chain
  /// (chance -> reveal -> teleport) reads as separate moments. They converge on
  /// the authoritative view once the plan finishes.
  final Map<int, int> pawnAt = {};

  /// "Glide straight, do not hop": a teleport, or a "do not pass Go" card that
  /// must not appear to cross Go.
  final Map<int, bool> glide = {};

  /// "Hop the whole way even though it is long": a card that wraps forward
  /// through Go *does* collect salary, so it must be seen crossing Go.
  final Map<int, bool> forceHop = {};

  /// The pawn layer runs the glide the director paid for, so it must scale by
  /// the same factor the plan was compressed (or the profile scaled) by -
  /// otherwise the beat ends while the pawn is still sliding, which is exactly
  /// the drift the old two-sources-of-truth design produced. 0 = snap.
  double hopScale = 1;

  // -- travelling primitives ------------------------------------------------

  final List<Chit> chits = [];
  int _chitId = 0;

  void addChit({
    required Anchor from,
    required Anchor to,
    required String text,
    required ChitKind kind,
    bool amplified = false,
  }) {
    chits.add(Chit(
      id: _chitId++,
      from: from,
      to: to,
      text: text,
      kind: kind,
      travels: profile.travels,
      amplified: amplified,
    ));
    notifyListeners();
  }

  /// Called by the overlay when a chit's own animation has run out.
  void retireChit(int id) {
    chits.removeWhere((c) => c.id == id);
    notifyListeners();
  }

  // -- attention ------------------------------------------------------------
  //
  // The camera never moves (`motion-language.md` 2). Attention is these three,
  // and nothing else.

  /// Lifted: "act on this tile" (P2).
  int? focusTile;

  /// Everything except the subject drops back: "nothing else matters right
  /// now" (P1, and P2 for the sealed bid only). The strongest instrument in
  /// the game - four uses in a typical match.
  bool recede = false;

  /// Framed: "this tile is the subject" (P3).
  int? frameTile;

  /// Struck once, oxblood: something was done to this tile.
  final Set<int> threatTiles = {};

  /// Tiles whose band is currently sweeping to a new owner, coalesced across a
  /// portfolio so an estate changing hands is one motion, not a drip.
  final Map<int, int> sweeping = {};

  // -- one-shot reveals -----------------------------------------------------
  //
  // `seq` retriggers the widget even when the payload repeats.

  int cardSeq = 0;
  int cardValue = 0;

  int bannerSeq = 0;
  String bannerText = '';
  BannerKind bannerKind = BannerKind.card;

  BidReveal? bidReveal;
  Arrest? arrest;

  /// The subject that just refused an action, and why.
  ///
  /// An error belongs on the thing that said no - never in a modal, never as a
  /// line in a log the player is not reading. "No" is a physical gesture, which
  /// is why this is the one place in the whole game a lateral shake is allowed.
  /// `refuseTile` is null when the rejected command was not about a tile.
  int refuseSeq = 0;
  int? refuseTile;
  String refuseText = '';

  void refuse(int? tile, String reason) {
    refuseTile = tile;
    refuseText = reason;
    refuseSeq++;
    notifyListeners();
  }

  // -- skipping -------------------------------------------------------------

  /// True once the player has asked to skip the plan in flight: the remaining
  /// beats apply immediately instead of being waited for. Motion never gates
  /// input, and a player who has seen enough is allowed to say so.
  bool skipping = false;

  DateTime? _arrestAt;

  /// A P1 must be *seen* before it can be dismissed - the information beat is
  /// not skippable, only the hold that follows it.
  bool get canSkip =>
      _arrestAt == null ||
      DateTime.now().difference(_arrestAt!) >= Motion.arrestFloor;

  void requestSkip() {
    if (!canSkip || skipping) return;
    skipping = true;
    notifyListeners();
  }

  void beginPlan() {
    skipping = false;
    _arrestAt = null;
  }

  void markArrest() => _arrestAt = DateTime.now();

  // -- lifecycle ------------------------------------------------------------

  void bump() => notifyListeners();

  /// Snap to authoritative truth and drop everything in flight. A reconnecting
  /// or room-switching client renders the present, never a replay of the past.
  void reset(List<int> positions) {
    pawnAt
      ..clear()
      ..addEntries(positions.asMap().entries);
    glide.clear();
    forceHop.clear();
    chits.clear();
    sweeping.clear();
    threatTiles.clear();
    focusTile = null;
    frameTile = null;
    recede = false;
    bidReveal = null;
    arrest = null;
    notifyListeners();
  }

  /// Converge on the view once a plan has finished playing.
  void syncPositions(List<int> positions) {
    for (var i = 0; i < positions.length; i++) {
      pawnAt[i] = positions[i];
    }
    sweeping.clear();
    threatTiles.clear();
    notifyListeners();
  }

  /// Reconcile the attention devices with the authoritative view once a plan
  /// has finished.
  ///
  /// A P2 decision is a *mode*: the lift and the recede persist for as long as
  /// the window is open, because the player is still deciding. A P1 is a
  /// *moment*: it ends with its beat, and the persistent consequence is carried
  /// by the ordinary UI (the winner card, the greyed-out seat) rather than by a
  /// scrim that would block the player from clicking anything.
  ///
  /// Deriving both from the view rather than from what the beats happened to
  /// leave behind is also what makes a reconnect correct for free.
  void settle({int? decisionTile}) {
    arrest = null;
    skipping = false;
    _arrestAt = null;
    focusTile = decisionTile;
    recede = decisionTile != null;
    notifyListeners();
  }
}
