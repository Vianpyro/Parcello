/// Dart mirror of the `parcello-protocol` wire shapes (JSON over WebSocket,
/// snake_case, type-tagged). The server is the only authority; these types
/// are read-only projections. Commands are sent as plain maps, exactly like
/// the reference web client, so the wire format stays visible at call sites.
library;

class SeatInfo {
  final int seat;
  final String playerId;
  final String name;
  final bool connected;
  final bool isBot;

  SeatInfo.fromJson(Map<String, dynamic> j)
      : seat = j['seat'] as int,
        playerId = j['player_id'] as String,
        name = j['name'] as String,
        connected = j['connected'] as bool,
        isBot = j['is_bot'] as bool? ?? false;
}

/// One board tile definition. Property fields are null for non-properties;
/// `amount` is only set for tax tiles, `minPct`/`maxPct` only for
/// net-worth tax tiles (ADR-0029).
class TileDef {
  final String id;
  final String name;
  final String kind; // go | property | chance | community | tax | jail | ...
  final String? group;
  final int? price;
  final int houseCost;
  final String rentModel; // meaningful only when kind == property
  final int? amount;
  final int? minPct;
  final int? maxPct;

  TileDef.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        kind = j['kind']['type'] as String,
        group = j['kind']['group'] as String?,
        price = j['kind']['price'] as int?,
        houseCost = j['kind']['house_cost'] as int? ?? 0,
        rentModel = j['kind']['rent_model'] as String? ?? 'houses',
        amount = j['kind']['amount'] as int?,
        minPct = j['kind']['min_pct'] as int?,
        maxPct = j['kind']['max_pct'] as int?;

  bool get isProperty => kind == 'property';
}

/// A market event definition (ADR-0021): `id` ties it back to a
/// `ScheduledEvent`/`ActiveMarketEvent` on the view.
class MarketEventDef {
  final String id;
  final String name;
  final String effect; // rent_multiplier | acquisition_multiplier | wealth_tax
  final int magnitudePct;
  final int durationTurns;

  MarketEventDef.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        effect = j['effect'] as String,
        magnitudePct = j['magnitude_pct'] as int,
        durationTurns = j['duration_turns'] as int;
}

class GameContent {
  final List<TileDef> board;
  final List<String> modIds;
  final List<MarketEventDef> marketEvents;

  /// Rule knobs the clients need to gate UI (ADR-0011/0012): cost percents,
  /// 0 = mechanic disabled.
  final int expropriation;
  final int rentBoost;
  /// Victory-point race target (ADR-0020); 0 = off.
  final int winVictoryPoints;
  /// The Exposition corner's spotlight rent bonus percent (ADR-0026); 0 =
  /// off.
  final int spotlightRentPct;

  GameContent.fromJson(Map<String, dynamic> resolved)
      : board = (resolved['content']['board'] as List)
            .map((t) => TileDef.fromJson(t as Map<String, dynamic>))
            .toList(),
        marketEvents = (resolved['content']['market_events'] as List? ?? [])
            .map((e) => MarketEventDef.fromJson(e as Map<String, dynamic>))
            .toList(),
        expropriation =
            resolved['content']['rules']['expropriation'] as int? ?? 0,
        rentBoost = resolved['content']['rules']['rent_boost'] as int? ?? 0,
        winVictoryPoints =
            resolved['content']['rules']['win_victory_points'] as int? ?? 0,
        spotlightRentPct =
            resolved['content']['rules']['spotlight_rent_pct'] as int? ?? 0,
        modIds = (resolved['mods'] as List)
            .map((m) => m['id'] as String)
            .toList();

  String marketEventName(String eventId) {
    for (final e in marketEvents) {
      if (e.id == eventId) return e.name;
    }
    return eventId;
  }
}

/// Mirror of the engine `RuleParams` (ADR-0015). Absolute values; the host
/// edits them in the lobby and the server clamps.
class RuleParams {
  final int startingBalance;
  final int goSalary;
  /// Velocity deck range (ADR-0017): movement is playing a card from a
  /// public hand of every integer in `velocityMin..=velocityMax`, not
  /// rolling dice. Also sizes a Legal Route (ADR-0024).
  final int velocityMin;
  final int velocityMax;
  final int maxHousesPerProperty;
  final int bankruptcyThreshold;
  final int expropriation;
  final int rentBoost;
  final int winFullGroups;
  /// Race-to-target victory points (ADR-0020); 0 disables. Also gates the
  /// round bonus and the conglomerate-pool "doom clock".
  final int winVictoryPoints;
  /// Shared building pool sizing factors (ADR-0019); 0 disables pooling.
  final int subsidiaryPoolFactor;
  final int conglomeratePoolFactor;
  /// The Exposition corner's spotlight (ADR-0026): rent bonus percent and
  /// duration in turns; 0/0 disables (no `Spotlight` tile ever triggers it
  /// regardless).
  final int spotlightRentPct;
  final int spotlightDurationTurns;

  RuleParams.fromJson(Map<String, dynamic> j)
      : startingBalance = j['starting_balance'] as int,
        goSalary = j['go_salary'] as int,
        velocityMin = j['velocity_min'] as int? ?? 1,
        velocityMax = j['velocity_max'] as int? ?? 5,
        maxHousesPerProperty = j['max_houses_per_property'] as int,
        bankruptcyThreshold = j['bankruptcy_threshold'] as int,
        expropriation = j['expropriation'] as int? ?? 0,
        rentBoost = j['rent_boost'] as int? ?? 0,
        winFullGroups = j['win_full_groups'] as int? ?? 0,
        winVictoryPoints = j['win_victory_points'] as int? ?? 0,
        subsidiaryPoolFactor = j['subsidiary_pool_factor'] as int? ?? 0,
        conglomeratePoolFactor = j['conglomerate_pool_factor'] as int? ?? 0,
        spotlightRentPct = j['spotlight_rent_pct'] as int? ?? 0,
        spotlightDurationTurns = j['spotlight_duration_turns'] as int? ?? 0;
}

/// Per-room settings the host edits in the lobby (ADR-0015).
class RoomSettings {
  final int? gameSeconds;
  final int? turnSeconds;
  /// Personal time bank in seconds (ADR-0023); `null`/0 disables it.
  final int? timeBankSeconds;
  final RuleParams rules;

  RoomSettings.fromJson(Map<String, dynamic> j)
      : gameSeconds = j['game_seconds'] as int?,
        turnSeconds = j['turn_seconds'] as int?,
        timeBankSeconds = j['time_bank_seconds'] as int?,
        rules = RuleParams.fromJson(j['rules'] as Map<String, dynamic>);
}

class PlayerView {
  final String id;
  final String name;
  final int cash;
  final int position;
  final bool inJail;
  final int jailCards;
  final bool bankrupt;
  /// Race-to-target score (ADR-0020); meaningless (always 0) when
  /// `RuleParams.winVictoryPoints` is off.
  final int victoryPoints;
  /// Movement values currently held (ADR-0017); public like cash, never
  /// masked.
  final List<int> hand;
  /// `Some(queue)` while serving a locked, public Legal Route (ADR-0024) -
  /// transparency is the price of the immediate exit and rent freeze.
  final List<int>? jailRoute;
  /// Hands fully cycled (ADR-0020's round metronome): the round number is
  /// the minimum of this across surviving players, and the +2 round bonus
  /// fires when the last straggler refills and lifts that minimum.
  final int handsCycled;

  PlayerView.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        cash = j['cash'] as int,
        position = j['position'] as int,
        inJail = j['in_jail'] as bool,
        jailCards = j['jail_cards'] as int? ?? 0,
        bankrupt = j['bankrupt'] as bool,
        victoryPoints = j['victory_points'] as int? ?? 0,
        hand = (j['hand'] as List? ?? []).cast<int>(),
        jailRoute = (j['jail_route'] as List?)?.cast<int>(),
        handsCycled = j['hands_cycled'] as int? ?? 0;
}

class TileState {
  final int? owner;
  final int houses;
  final bool mortgaged;
  final int boosts;

  TileState.fromJson(Map<String, dynamic> j)
      : owner = j['owner'] as int?,
        houses = j['houses'] as int,
        mortgaged = j['mortgaged'] as bool? ?? false,
        boosts = j['boosts'] as int? ?? 0;
}

/// Flattened turn phase: `type` selects which of the nullable fields apply.
/// `blind_auction` (ADR-0018) is a sealed-bid window open to every living
/// seat at once, not a single actor: `bids` is one slot per seat, `null` =
/// not yet submitted; a seat's own view shows its own bid, others' are
/// masked to `null` while the window is open (server-side secrecy).
/// `bribe_vote` (ADR-0024) is the same pattern for a Corruption bribe:
/// `briber`/`amount` name the offer, `votes` is one slot per seat (the
/// briber's own slot always stays `null`), individual votes masked the
/// same way as sealed bids until resolution.
class TurnPhase {
  final String type; // await_move | blind_auction | bribe_vote | await_end
  final int? tile;
  final List<int?> bids;
  final int? briber;
  final int? amount;
  final List<bool?> votes;

  TurnPhase.fromJson(Map<String, dynamic> j)
      : type = j['type'] as String,
        tile = j['tile'] as int?,
        bids = (j['bids'] as List<dynamic>? ?? [])
            .map((b) => b as int?)
            .toList(),
        briber = j['briber'] as int?,
        amount = j['amount'] as int?,
        votes = (j['votes'] as List<dynamic>? ?? [])
            .map((v) => v as bool?)
            .toList();
}

class TradeOffer {
  final int id;
  final int from;
  final int to;
  final int giveCash;
  final List<int> giveTiles;
  final int receiveCash;
  final List<int> receiveTiles;

  TradeOffer.fromJson(Map<String, dynamic> j)
      : id = j['id'] as int,
        from = j['from'] as int,
        to = j['to'] as int,
        giveCash = j['give_cash'] as int,
        giveTiles = (j['give_tiles'] as List).cast<int>(),
        receiveCash = j['receive_cash'] as int,
        receiveTiles = (j['receive_tiles'] as List).cast<int>();
}

/// A drawn-but-not-yet-active market event (ADR-0021).
class ScheduledEvent {
  final String eventId;
  final int startsAtTurn;
  final int duration;

  ScheduledEvent.fromJson(Map<String, dynamic> j)
      : eventId = j['event_id'] as String,
        startsAtTurn = j['starts_at_turn'] as int,
        duration = j['duration'] as int;
}

/// The market event currently in effect, if any (ADR-0021).
class ActiveMarketEvent {
  final String eventId;
  final String effect;
  final int magnitudePct;
  final int endsAtTurn;

  ActiveMarketEvent.fromJson(Map<String, dynamic> j)
      : eventId = j['event_id'] as String,
        effect = j['effect'] as String,
        magnitudePct = j['magnitude_pct'] as int,
        endsAtTurn = j['ends_at_turn'] as int;
}

/// Public market forecast queue (ADR-0021).
class MarketForecast {
  final List<ScheduledEvent> queue;
  final ActiveMarketEvent? active;

  MarketForecast.fromJson(Map<String, dynamic>? j)
      : queue = (j?['queue'] as List? ?? [])
            .map((s) => ScheduledEvent.fromJson(s as Map<String, dynamic>))
            .toList(),
        active = j?['active'] != null
            ? ActiveMarketEvent.fromJson(j!['active'] as Map<String, dynamic>)
            : null;
}

/// The property currently in the Exposition corner's spotlight (ADR-0026),
/// if any - fully public, never masked per-seat.
class Spotlight {
  final int tile;
  final int expiresAtTurn;

  Spotlight.fromJson(Map<String, dynamic> j)
      : tile = j['tile'] as int,
        expiresAtTurn = j['expires_at_turn'] as int;
}

class ClientView {
  final bool finished;
  final int? winner;
  final List<PlayerView> players;
  final int current;
  final TurnPhase turn;
  final List<TileState> tiles;
  final List<TradeOffer> pendingTrades;
  /// Shared building pools (ADR-0019); `null` = unlimited (pooling off).
  final int? subsidiariesAvailable;
  final int? conglomeratesAvailable;
  final MarketForecast forecast;
  /// The Exposition corner's current spotlight (ADR-0026), if any.
  final Spotlight? spotlight;

  ClientView.fromJson(Map<String, dynamic> j)
      : finished = j['phase']['type'] == 'finished',
        winner = j['phase']['winner'] as int?,
        players = (j['players'] as List)
            .map((p) => PlayerView.fromJson(p as Map<String, dynamic>))
            .toList(),
        current = j['current'] as int,
        turn = TurnPhase.fromJson(j['turn'] as Map<String, dynamic>),
        tiles = (j['tiles'] as List)
            .map((t) => TileState.fromJson(t as Map<String, dynamic>))
            .toList(),
        pendingTrades = (j['pending_trades'] as List? ?? [])
            .map((t) => TradeOffer.fromJson(t as Map<String, dynamic>))
            .toList(),
        subsidiariesAvailable = j['subsidiaries_available'] as int?,
        conglomeratesAvailable = j['conglomerates_available'] as int?,
        forecast = MarketForecast.fromJson(j['forecast'] as Map<String, dynamic>?),
        spotlight = j['spotlight'] != null
            ? Spotlight.fromJson(j['spotlight'] as Map<String, dynamic>)
            : null;
}

String _identityEventName(String id) => id;

/// Human-readable line for one engine event (the animation/log feed).
/// Ported verbatim from the reference web client's `describe`. `m` looks up
/// a market event's display name (ADR-0021); optional so existing callers
/// (and tests) that don't have content loaded yet still get the raw id.
String describeEvent(
  Map<String, dynamic> e,
  String Function(int seat) p,
  String Function(int tile) t, [
  String Function(String eventId) m = _identityEventName,
]) {
  switch (e['type']) {
    case 'turn_started':
      return "--- ${p(e['player'])}'s turn ---";
    case 'movement_card_played':
      return "${p(e['player'])} played movement card ${e['value']}";
    case 'moved':
      return "${p(e['player'])} moved to ${t(e['to'])}"
          "${e['passed_go'] == true ? ' (passed Go)' : ''}";
    case 'salary_paid':
      return "${p(e['player'])} collected \$${e['amount']} salary";
    case 'blind_auction_opened':
      return "${p(e['discoverer'])} landed on ${t(e['tile'])}: sealed bid open "
          "(\$${e['floor']} floor for ${p(e['discoverer'])})";
    case 'blind_bid_submitted':
      return "${p(e['player'])} submitted a bid";
    case 'blind_auction_resolved':
      return e['winner'] == null
          ? "${t(e['tile'])} stays unsold"
          : "${p(e['winner'])} won ${t(e['tile'])} at \$${e['amount']}";
    case 'rent_paid':
      return "${p(e['from'])} paid \$${e['amount']} rent to ${p(e['to'])} for ${t(e['tile'])}";
    case 'tax_paid':
      return "${p(e['player'])} paid \$${e['amount']} tax";
    case 'card_drawn':
      return "${p(e['player'])} drew: ${e['text']}";
    case 'cash_adjusted':
      final int delta = e['delta'];
      return "${p(e['player'])} ${delta >= 0 ? 'received' : 'paid'} \$${delta.abs()} (${e['reason']})";
    case 'house_built':
      return "${p(e['player'])} built on ${t(e['tile'])} (now ${e['houses']})";
    case 'house_sold':
      return "${p(e['player'])} sold a house on ${t(e['tile'])} (+\$${e['refund']})";
    case 'expropriated':
      final base =
          "${p(e['player'])} seized ${t(e['tile'])} from ${p(e['from'])} for \$${e['cost']}";
      final liquidated = e['liquidated'] as int? ?? 0;
      return liquidated > 0
          ? "$base ($liquidated levels liquidated, \$${e['liquidation_refund']} to the former owner)"
          : base;
    case 'rent_boosted':
      return "${p(e['player'])} boosted ${t(e['tile'])} rent to level ${e['boosts']} for \$${e['cost']}";
    case 'property_mortgaged':
      return "${p(e['player'])} mortgaged ${t(e['tile'])} for \$${e['value']}";
    case 'property_unmortgaged':
      return "${p(e['player'])} redeemed ${t(e['tile'])} for \$${e['cost']}";
    case 'went_to_jail':
      return "${p(e['player'])} went to jail";
    case 'legal_route_chosen':
      final order = (e['order'] as List).join(',');
      return "${p(e['player'])} chose a Legal Route [$order] "
          "(rent-free on their tiles until it's done)";
    case 'bribe_offered':
      return "${p(e['player'])} offers a \$${e['amount']} bribe to leave jail";
    case 'bribe_vote_cast':
      return "${p(e['player'])} voted on the bribe";
    case 'bribe_resolved':
      return e['succeeded'] == true
          ? "Bribe accepted (${e['accepts']}/${e['total']}): "
              "${p(e['briber'])} pays \$${e['amount']}, split among the table"
          : "Bribe rejected (${e['accepts']}/${e['total']}): "
              "${p(e['briber'])} stays in jail";
    case 'jail_card_received':
      return "${p(e['player'])} received a get-out-of-jail-free card";
    case 'jail_card_used':
      return "${p(e['player'])} used a get-out-of-jail-free card";
    case 'left_jail':
      return "${p(e['player'])} left jail";
    case 'property_transferred':
      return e['to'] == null
          ? "${t(e['tile'])} returned to the bank"
          : "${t(e['tile'])} transferred to ${p(e['to'])}";
    case 'trade_proposed':
      return "${p(e['from'])} proposed trade #${e['trade']} to ${p(e['to'])}";
    case 'trade_accepted':
      return "${p(e['to'])} accepted trade #${e['trade']}";
    case 'trade_declined':
      return "Trade #${e['trade']} declined";
    case 'trade_cancelled':
      return "Trade #${e['trade']} cancelled";
    case 'player_bankrupt':
      return "${p(e['player'])} went bankrupt";
    case 'player_resigned':
      return "${p(e['player'])} resigned";
    case 'game_ended':
      return "Game over -- ${p(e['winner'])} wins!";
    case 'time_up':
      return "Time's up! ${p(e['winner'])} wins on net worth.";
    case 'won_by_groups':
      return "${p(e['winner'])} wins by controlling ${e['groups']} colour groups!";
    case 'won_by_points':
      return "${p(e['player'])} wins with ${e['points']} victory points!";
    case 'won_by_pool_exhaustion':
      return "The conglomerate pool ran dry -- ${p(e['winner'])} wins on victory points!";
    case 'market_event_activated':
      final pct = e['magnitude_pct'] as int;
      final sign = pct > 0 ? '+' : '';
      final duration = e['duration_turns'] as int;
      return duration == 0
          ? "Market event: ${m(e['event_id'] as String)} ($sign$pct%)"
          : "Market event: ${m(e['event_id'] as String)} ($sign$pct% for $duration turns)";
    case 'market_event_expired':
      return "Market event ended: ${m(e['event_id'] as String)}";
    case 'spotlight_started':
      final pct = e['rent_pct'] as int;
      final duration = e['duration_turns'] as int;
      final span = duration <= 0
          ? 'until the next Exposition landing'
          : 'for $duration turns';
      return "The Exposition spotlights ${t(e['tile'])} (+$pct% rent $span)";
    case 'spotlight_ended':
      return "The spotlight on ${t(e['tile'])} fades";
    case 'rent_boost_consumed':
      return "The boost on ${t(e['tile'])} is spent (one-shot trap)";
    case 'round_bonus_awarded':
      return "${p(e['player'])} is the round's cash leader: "
          "+${e['points']} permanent VP";
    default:
      return e.toString();
  }
}
