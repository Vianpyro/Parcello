/// The animation director (ADR-0028, ADR-0030): turns one server `Update`
/// into a *plan* of beats, then plays it.
///
/// The split that matters is `compile` vs `execute`. `compile` is a pure
/// function of (events, context) -> Plan: no socket, no widgets, no clock. It
/// is where the tier, the lane, the coalescing and - above all - the budget
/// are decided, and it is why "no plan exceeds ANIM_BUDGET" and "a bankruptcy
/// coalesces" are assertions in a unit test rather than hopes.
///
/// The budget is not a nicety. The server un-gates its timers at
/// `ANIM_ACK_CAP` = 6s (ADR-0028); a client whose beats outrun that is not
/// slow, it is *behind the game* - the bid window opens while it is still
/// animating the previous turn. So the whole Update is costed before a single
/// frame is shown, and an over-budget plan is compressed, never played long.
library;

import 'motion.dart';
import 'stage.dart';

/// Everything `compile` needs to know about the world. A plain value, so the
/// compiler stays pure - and so a test can build one in a line.
class CompileCtx {
  final int boardLen;

  /// -1 when the board has no jail tile (a mod may not).
  final int jailTile;
  final int? mySeat;

  /// Seat -> tile, as currently *displayed* (not as the server says): a beat
  /// animates from where the pawn visually is.
  final Map<int, int> positions;
  final String Function(int tile) tileName;
  final String Function(int seat) playerName;

  /// The accessibility knob (ADR-0030). It is part of the compile input, not a
  /// global the executor reads later: an instant plan must *cost* nothing, and
  /// cost is decided here.
  final MotionProfile profile;

  const CompileCtx({
    required this.boardLen,
    required this.jailTile,
    required this.mySeat,
    required this.positions,
    required this.tileName,
    required this.playerName,
    this.profile = MotionProfile.full,
  });
}

/// A compiled Update. Its cost is known before the first frame - which is what
/// makes the budget enforceable rather than discovered halfway through.
class Plan {
  final List<Beat> beats;
  const Plan(this.beats);

  /// Wall-clock the plan will take. Exclusive beats add up; concurrent beats
  /// ride alongside and only cost whatever of them outlasts the last exclusive
  /// beat.
  Duration get cost {
    var total = Duration.zero;
    var tail = Duration.zero;
    for (final b in beats) {
      if (b.lane == Lane.exclusive) {
        total += b.cost;
        tail = Duration.zero;
      } else if (b.cost > tail) {
        tail = b.cost;
      }
    }
    return total + tail;
  }
}

/// One visual beat.
///
/// `apply` must be meaningful with zero duration: that is what makes the
/// instant profile a first-class path (ADR-0030) rather than an information
/// loss, and what lets the budget compressor zero a beat without losing state.
sealed class Beat {
  Tier get tier;
  Lane get lane;
  Duration get cost;
  void apply(StageState s);

  /// A copy costing `factor` of this one. `0` keeps the state change and drops
  /// the journey entirely.
  Beat scaled(double factor);
}

// -- beats -------------------------------------------------------------------

final class MoveBeat extends Beat {
  final int seat, from, to, boardLen;
  final bool passedGo;
  final double f;
  MoveBeat(this.seat, this.from, this.to, this.passedGo, this.boardLen,
      [this.f = 1]);

  /// A forward wrap that collected salary always crossed Go, so it must be
  /// *seen* crossing Go or the +$salary chit has no visible cause. A wrap
  /// without salary ("do not pass Go") is the opposite: it must glide straight
  /// or it appears to cross Go and promise a salary that never comes. Both are
  /// the truth rule: motion may not imply a path the engine did not take.
  bool get _wraps =>
      boardLen > 0 && from + ((to - from) % boardLen) >= boardLen;
  bool get straight => _wraps && !passedGo;
  bool get forceHop => _wraps && passedGo;

  int get _forward => boardLen == 0 ? 0 : (to - from) % boardLen;

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.exclusive;

  @override
  Duration get cost {
    if (boardLen == 0) return Duration.zero;
    final hops = !straight &&
        (forceHop || (_forward >= 1 && _forward <= Motion.hopTaperFrom));
    final travel =
        hops ? Motion.hop(_forward).inMilliseconds : Motion.glide.inMilliseconds;
    return Duration(
        milliseconds:
            ((Motion.hopWindUp.inMilliseconds + travel + 150) * f).round());
  }

  @override
  void apply(StageState s) {
    s.glide[seat] = straight;
    s.forceHop[seat] = forceHop;
    s.hopScale = f;
    s.pawnAt[seat] = to;
    s.frameTile = to;
    s.bump();
  }

  @override
  Beat scaled(double factor) =>
      MoveBeat(seat, from, to, passedGo, boardLen, f * factor);
}

final class CardPlayBeat extends Beat {
  final int value;
  final double f;
  CardPlayBeat(this.value, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost => Motion.cardPlay * f;

  @override
  void apply(StageState s) {
    s.cardValue = value;
    s.cardSeq++;
    s.bump();
  }

  @override
  Beat scaled(double factor) => CardPlayBeat(value, f * factor);
}

final class BannerBeat extends Beat {
  final String text;
  final BannerKind kind;
  final double f;
  BannerBeat(this.text, this.kind, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost =>
      (kind == BannerKind.card ? Motion.cardReveal : Motion.banner) * f;

  @override
  void apply(StageState s) {
    s.bannerText = text;
    s.bannerKind = kind;
    s.bannerSeq++;
    s.bump();
  }

  @override
  Beat scaled(double factor) => BannerBeat(text, kind, f * factor);
}

final class JailBeat extends Beat {
  final int seat, from, jailTile;
  final double f;
  JailBeat(this.seat, this.from, this.jailTile, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost => Motion.jail * f;

  @override
  void apply(StageState s) {
    // A teleport: the pawn did not walk there.
    s.glide[seat] = true;
    s.forceHop[seat] = false;
    s.hopScale = f;
    if (jailTile >= 0) s.pawnAt[seat] = jailTile;
    s.bump();
  }

  @override
  Beat scaled(double factor) => JailBeat(seat, from, jailTile, f * factor);
}

/// Money in flight. The whole point of the money rule: a rent payment is ONE
/// object leaving the payer and landing on the owner, so "who paid whom" is
/// the shape of the motion and never something the player has to work out.
final class ChitBeat extends Beat {
  final Anchor from, to;
  final String text;
  final ChitKind kind;
  final bool amplified;
  final double f;
  ChitBeat({
    required this.from,
    required this.to,
    required this.text,
    required this.kind,
    this.amplified = false,
    this.f = 1,
  });

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.concurrent;
  @override
  Duration get cost => Motion.chit * f;

  @override
  void apply(StageState s) {
    s.addChit(
        from: from, to: to, text: text, kind: kind, amplified: amplified);
  }

  @override
  Beat scaled(double factor) => ChitBeat(
      from: from,
      to: to,
      text: text,
      kind: kind,
      amplified: amplified,
      f: f * factor);
}

/// P2: a window is open. Recede the board, lift the subject, anchor the input
/// to it. The animation only transports the player into the decision - it ends,
/// and a persistent mode remains.
final class FocusBeat extends Beat {
  final int tile;
  final bool recede;
  final double f;
  FocusBeat(this.tile, {this.recede = true, this.f = 1});

  @override
  Tier get tier => Tier.decide;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost => Motion.establish * f;

  @override
  void apply(StageState s) {
    s.focusTile = tile;
    s.recede = recede;
    s.frameTile = null;
    s.bump();
  }

  @override
  Beat scaled(double factor) =>
      FocusBeat(tile, recede: recede, f: f * factor);
}

/// The sealed bids, face-up and held long enough to compare (ADR-0018). The
/// most information-dense moment in the game; the old client never showed it.
final class BidRevealBeat extends Beat {
  final BidReveal reveal;
  final double f;
  BidRevealBeat(this.reveal, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost => Motion.bidReveal * f;

  @override
  void apply(StageState s) {
    s.bidReveal = reveal;
    s.focusTile = reveal.tile;
    s.recede = false;
    s.bump();
  }

  @override
  Beat scaled(double factor) => BidRevealBeat(reveal, f * factor);
}

/// Ownership is the tile's band. It changes by sweeping, and a whole portfolio
/// sweeps at once - an 18-tile bankruptcy and a 2-tile one take the same time,
/// because the information ("X is out, Y took everything") is the same; only
/// how much of the board changes colour differs.
final class BandSweepBeat extends Beat {
  /// tile -> new owner seat (-1 = back to the bank).
  final Map<int, int> tiles;
  final double f;
  BandSweepBeat(this.tiles, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.concurrent;

  @override
  Duration get cost =>
      (Motion.bandSweep + Motion.stagger * (tiles.length - 1)) * f;

  @override
  void apply(StageState s) {
    s.sweeping.addAll(tiles);
    s.bump();
  }

  @override
  Beat scaled(double factor) => BandSweepBeat(tiles, f * factor);
}

/// Something was done *to* this tile. Snaps in without a ramp and lingers.
final class ThreatBeat extends Beat {
  final int tile;
  final double f;
  ThreatBeat(this.tile, [this.f = 1]);

  @override
  Tier get tier => Tier.consequence;
  @override
  Lane get lane => Lane.concurrent;
  @override
  Duration get cost => Motion.refuse * f;

  @override
  void apply(StageState s) {
    s.threatTiles.add(tile);
    s.bump();
  }

  @override
  Beat scaled(double factor) => ThreatBeat(tile, f * factor);
}

/// P1. The board recedes; the table stops. Never compressed: if a bankruptcy
/// and a 4-deep card chain land in the same Update, the card chain is what
/// gets cut.
final class ArrestBeat extends Beat {
  final Arrest arrest;
  final bool win;
  ArrestBeat(this.arrest, {this.win = false});

  @override
  Tier get tier => Tier.arrest;
  @override
  Lane get lane => Lane.exclusive;
  @override
  Duration get cost => win ? Motion.arrestWin : Motion.arrest;

  @override
  void apply(StageState s) {
    s.arrest = arrest;
    s.recede = true;
    s.focusTile = null;
    s.bump();
  }

  // Deliberately ignores the factor: P1 is never compressed.
  @override
  Beat scaled(double factor) => this;
}

// -- the compiler ------------------------------------------------------------

/// Events that coalesce across a whole Update rather than playing one by one.
const _coalesced = {'property_transferred', 'house_sold'};

Plan compile(List<Map<String, dynamic>> events, CompileCtx ctx) {
  final beats = <Beat>[];
  final seen = <String>{};

  // A one-shot boost trap (ADR-0012) springs in the same Update as the rent it
  // inflates, and until now it sprang silently: the victim saw a large number
  // and no reason for it. Knowing which tiles fired lets the rent chit *grow*
  // as it crosses them - the causal link between "the trap fired" and "that
  // number is huge". Answering "why did that happen?" is the whole job.
  final sprung = {
    for (final e in events)
      if (e['type'] == 'rent_boost_consumed') e['tile'] as int,
  };

  for (final e in events) {
    final type = e['type'] as String;

    // One beat per coalesced kind, placed where the first one landed; the rest
    // fold into it. This is a readability rule, not an optimisation: a
    // bankruptcy's portfolio is one event, not eight.
    if (_coalesced.contains(type)) {
      if (!seen.add(type)) continue;
      final beat = _coalesce(type, events, ctx);
      if (beat != null) beats.add(beat);
      continue;
    }

    beats.addAll(_beatsFor(e, ctx, sprung));
  }

  return _fit(beats, ctx.profile.scale);
}

/// Pull every event of `type` out of the Update and fold them into one beat.
Beat? _coalesce(
    String type, List<Map<String, dynamic>> events, CompileCtx ctx) {
  final all = events.where((e) => e['type'] == type);
  switch (type) {
    case 'property_transferred':
      final tiles = <int, int>{
        for (final e in all) e['tile'] as int: (e['to'] as int?) ?? -1,
      };
      return tiles.isEmpty ? null : BandSweepBeat(tiles);
    case 'house_sold':
      // A forced liquidation strips a whole estate; one motion, not a drip.
      final refund = all.fold<int>(0, (a, e) => a + (e['refund'] as int));
      final seat = all.first['player'] as int;
      final tile = all.first['tile'] as int;
      final mine = ctx.mySeat == seat;
      return refund == 0
          ? null
          : ChitBeat(
              from: TileAnchor(tile),
              to: SeatAnchor(seat),
              text: '${mine ? '+' : ''}\$$refund',
              kind: mine ? ChitKind.gain : ChitKind.neutral);
    default:
      return null;
  }
}

List<Beat> _beatsFor(
    Map<String, dynamic> e, CompileCtx ctx, Set<int> sprungTraps) {
  final seatOf = ctx.positions;
  TileAnchor at(int seat) => TileAnchor(seatOf[seat] ?? 0);

  /// Money is typed per observer, not per event: the same rent chit is a loss
  /// to the payer, a gain to the owner and neither to the table. `payer`/`payee`
  /// of -1 mean "the bank", which is nobody's seat.
  ChitBeat cash(
    int amount, {
    required Anchor from,
    required Anchor to,
    required int payer,
    required int payee,
    bool amplified = false,
  }) {
    final me = ctx.mySeat;
    final kind = me == payee
        ? ChitKind.gain
        : me == payer
            ? ChitKind.loss
            : ChitKind.neutral;
    final sign = switch (kind) {
      ChitKind.gain => '+',
      ChitKind.loss => '-',
      _ => '',
    };
    return ChitBeat(
        from: from,
        to: to,
        text: '$sign\$$amount',
        kind: kind,
        amplified: amplified);
  }

  switch (e['type'] as String) {
    case 'movement_card_played':
      return [CardPlayBeat(e['value'] as int)];

    case 'moved':
      final p = e['player'] as int;
      final from = e['from'] as int? ?? seatOf[p] ?? 0;
      return [
        MoveBeat(p, from, e['to'] as int, e['passed_go'] == true, ctx.boardLen)
      ];

    case 'went_to_jail':
      final p = e['player'] as int;
      final from = e['from'] as int? ?? seatOf[p] ?? 0;
      return [JailBeat(p, from, ctx.jailTile)];

    case 'card_drawn':
      return [BannerBeat(e['text'] as String? ?? '', BannerKind.card)];

    // -- money: it travels, always ----------------------------------------

    case 'salary_paid':
      final p = e['player'] as int;
      return [
        cash(e['amount'] as int,
            from: const TileAnchor(0), // Go
            to: SeatAnchor(p),
            payer: -1, // the bank
            payee: p),
      ];

    case 'rent_paid':
      // The one that mattered most. The old client floated only the payer's
      // loss, so the owner - who just earned the game's core income - saw
      // nothing at all. One chit, leaving the payer's pawn and landing on the
      // owner's marker: read as a loss from one seat, a gain from the other,
      // and "who paid whom" is never a question again.
      final from = e['from'] as int;
      final to = e['to'] as int;
      return [
        cash(e['amount'] as int,
            from: at(from),
            to: SeatAnchor(to),
            payer: from,
            payee: to,
            amplified: sprungTraps.contains(e['tile'] as int)),
      ];

    case 'tax_paid':
      final p = e['player'] as int;
      final tile = e['tile'] as int;
      return [
        cash(e['amount'] as int,
            from: at(p), to: TileAnchor(tile), payer: p, payee: -1),
        ThreatBeat(tile),
      ];

    case 'cash_adjusted':
      final p = e['player'] as int;
      final delta = e['delta'] as int;
      if (delta == 0) return const [];
      return [
        cash(delta.abs(),
            from: at(p),
            to: SeatAnchor(p),
            payer: delta > 0 ? -1 : p,
            payee: delta > 0 ? p : -1),
      ];

    // -- the core loop ------------------------------------------------------

    case 'blind_auction_opened':
      return [FocusBeat(e['tile'] as int)];

    case 'blind_auction_resolved':
      final winner = e['winner'] as int?;
      final tile = e['tile'] as int;
      final amount = e['amount'] as int;
      final bids = (e['bids'] as List? ?? const []).cast<int>();
      final top = bids.isEmpty ? 0 : bids.reduce((a, b) => a > b ? a : b);
      final beats = <Beat>[
        BidRevealBeat(BidReveal(
          tile: tile,
          bids: bids,
          winner: winner,
          amount: amount,
          // Won above the floor after a contest: the 90% discount shows as the
          // chit shrinking mid-flight - the discount is a thing that happens to
          // the money on its way.
          discounted: winner != null && top > 0 && amount < top,
        )),
      ];
      if (winner != null) {
        beats.add(cash(amount,
            from: SeatAnchor(winner),
            to: TileAnchor(tile),
            payer: winner,
            payee: -1));
        beats.add(BandSweepBeat({tile: winner}));
      }
      return beats;

    // -- aggression ---------------------------------------------------------

    case 'expropriated':
      final tile = e['tile'] as int;
      final by = e['player'] as int;
      final victim = e['from'] as int;
      final refund =
          (e['cost'] as int) + (e['liquidation_refund'] as int? ?? 0);
      return [
        ThreatBeat(tile),
        BandSweepBeat({tile: by}),
        cash(refund,
            from: SeatAnchor(by),
            to: SeatAnchor(victim),
            payer: by,
            payee: victim),
      ];

    case 'rent_boost_consumed':
      // The trap springs. It was armed turns ago and, until now, sprang
      // silently: the victim saw a large rent number and no reason for it.
      return [ThreatBeat(e['tile'] as int)];

    // -- estate -------------------------------------------------------------

    case 'house_built':
      final p = e['player'] as int;
      return [
        cash(e['cost'] as int,
            from: SeatAnchor(p),
            to: TileAnchor(e['tile'] as int),
            payer: p,
            payee: -1),
      ];

    case 'rent_boosted':
      final p = e['player'] as int;
      return [
        cash(e['cost'] as int,
            from: SeatAnchor(p),
            to: TileAnchor(e['tile'] as int),
            payer: p,
            payee: -1),
      ];

    case 'property_mortgaged':
      final p = e['player'] as int;
      return [
        cash(e['value'] as int,
            from: TileAnchor(e['tile'] as int),
            to: SeatAnchor(p),
            payer: -1,
            payee: p),
      ];

    case 'property_unmortgaged':
      final p = e['player'] as int;
      return [
        cash(e['cost'] as int,
            from: SeatAnchor(p),
            to: TileAnchor(e['tile'] as int),
            payer: p,
            payee: -1),
      ];

    // -- jail ---------------------------------------------------------------

    case 'bribe_offered':
      final p = e['player'] as int;
      return [FocusBeat(seatOf[p] ?? ctx.jailTile)];

    case 'bribe_resolved':
      if (e['succeeded'] != true) return const [];
      final briber = e['briber'] as int;
      return [BannerBeat('Bribe accepted', BannerKind.market), ThreatBeat(seatOf[briber] ?? ctx.jailTile)];

    // -- world --------------------------------------------------------------

    case 'spotlight_started':
      final pct = e['rent_pct'] as int;
      final turns = e['duration_turns'] as int;
      final span =
          turns <= 0 ? 'until the next Exposition landing' : 'for $turns turns';
      return [
        BannerBeat(
            '${ctx.tileName(e['tile'] as int)} is in the spotlight\n'
            '+$pct% rent $span',
            BannerKind.spotlight),
      ];

    case 'market_event_activated':
      final pct = e['magnitude_pct'] as int;
      final sign = pct > 0 ? '+' : '';
      return [
        BannerBeat('Market: ${e['event_id']} ($sign$pct%)', BannerKind.market),
      ];

    // -- P1: the table stops --------------------------------------------------

    case 'player_bankrupt':
      final p = e['player'] as int;
      final creditor = e['creditor'] as int?;
      return [
        ArrestBeat(Arrest(
          title: '${ctx.playerName(p)} is bankrupt',
          // Nobody inherits (ADR-0031): the estate goes back to the bank and
          // the board reopens. The creditor took the cash, and only the cash -
          // that is the whole message, and the table needs it immediately,
          // because every one of those tiles is about to be up for auction.
          detail: creditor == null
              ? 'The estate returns to the bank.'
              : 'The estate returns to the bank. '
                  '${ctx.playerName(creditor)} takes the cash.',
          seat: p,
        )),
      ];

    case 'game_ended':
      final w = e['winner'] as int;
      return [
        ArrestBeat(Arrest(title: '${ctx.playerName(w)} wins', seat: w),
            win: true),
      ];

    case 'won_by_points':
      final w = e['player'] as int;
      return [
        ArrestBeat(
            Arrest(
                title: '${ctx.playerName(w)} wins',
                detail: '${e['points']} victory points',
                seat: w),
            win: true),
      ];

    case 'won_by_groups':
      final w = e['winner'] as int;
      return [
        ArrestBeat(
            Arrest(
                title: '${ctx.playerName(w)} wins',
                detail: 'Domination: ${e['groups']} colour groups',
                seat: w),
            win: true),
      ];

    case 'won_by_pool_exhaustion':
      final w = e['winner'] as int;
      return [
        ArrestBeat(
            Arrest(
                title: '${ctx.playerName(w)} wins',
                detail: 'The conglomerate pool ran dry',
                seat: w),
            win: true),
      ];

    case 'time_up':
      final w = e['winner'] as int;
      return [
        ArrestBeat(
            Arrest(
                title: "Time's up",
                detail: '${ctx.playerName(w)} wins on net worth',
                seat: w),
            win: true),
      ];

    default:
      // P4: never a beat. The widget that owns the state transitions itself.
      return const [];
  }
}

// -- the budget --------------------------------------------------------------

/// Fit a plan into its budget (ADR-0030), compressing in a fixed order.
/// P1 beats are exempt at every stage.
Plan _fit(List<Beat> raw, double profileScale) {
  // The instant profile is a first-class path, not a degraded one: every beat's
  // apply() is meaningful with zero duration, so this loses no information.
  if (profileScale == 0) {
    return Plan([for (final b in raw) b.scaled(0)]);
  }

  final scaled =
      profileScale == 1 ? raw : [for (final b in raw) b.scaled(profileScale)];

  // The budget is set by the loudest beat in the Update: an Update carrying a
  // bankruptcy is a moment the table stops for and may take eight seconds; one
  // carrying only a move may not, because it happens every twelve.
  final budget = Motion.budgetFor(_loudest(scaled));

  var plan = Plan(scaled);
  if (plan.cost <= budget) return plan;

  // 1. Compress the non-P1 beats.
  for (final f in const [0.75, 0.55, 0.4]) {
    plan = Plan([
      for (final b in scaled) b.tier == Tier.arrest ? b : b.scaled(f)
    ]);
    if (plan.cost <= budget) return plan;
  }

  // 2. Still over: truncate the middle of the chain. The first beat says where
  //    it started, the last where it ended; the rest applies instantly and
  //    survives in the log. Zeroing a beat keeps its apply() - state is never
  //    lost, only its journey.
  final out = [
    for (final b in scaled) b.tier == Tier.arrest ? b : b.scaled(0.4)
  ];
  final droppable = [
    for (var i = 0; i < out.length; i++)
      if (out[i].lane == Lane.exclusive && out[i].tier != Tier.arrest) i,
  ];
  for (final i in _middleOut(droppable)) {
    out[i] = out[i].scaled(0);
    if (Plan(out).cost <= budget) break;
  }
  return Plan(out);
}

/// The highest-priority tier present. `Tier`'s declaration order is the
/// priority order (arrest first), so the loudest beat is the smallest index.
Tier _loudest(List<Beat> beats) => beats.fold(
      Tier.ambient,
      (worst, b) => b.tier.index < worst.index ? b.tier : worst,
    );

/// Indices ordered from the middle outward: the middle of a chain is the least
/// informative part of it, so it is the first thing to go. The first and last
/// beats are never candidates - they are where the chain started and ended.
List<int> _middleOut(List<int> xs) {
  if (xs.length <= 2) return const [];
  final mid = (xs.length - 1) / 2.0;
  final inner = [for (var i = 1; i < xs.length - 1; i++) i]
    ..sort((a, b) => (a - mid).abs().compareTo((b - mid).abs()));
  return [for (final i in inner) xs[i]];
}
