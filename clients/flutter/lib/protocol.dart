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
/// `amount` is only set for tax tiles.
class TileDef {
  final String id;
  final String name;
  final String kind; // go | property | chance | community | tax | jail | ...
  final String? group;
  final int? price;
  final int houseCost;
  final String rentModel; // meaningful only when kind == property
  final int? amount;

  TileDef.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        kind = j['kind']['type'] as String,
        group = j['kind']['group'] as String?,
        price = j['kind']['price'] as int?,
        houseCost = j['kind']['house_cost'] as int? ?? 0,
        rentModel = j['kind']['rent_model'] as String? ?? 'houses',
        amount = j['kind']['amount'] as int?;

  bool get isProperty => kind == 'property';
}

class GameContent {
  final List<TileDef> board;
  final List<String> modIds;

  /// Rule knobs the clients need to gate UI (ADR-0011/0012): cost percents,
  /// 0 = mechanic disabled.
  final int expropriation;
  final int rentBoost;

  GameContent.fromJson(Map<String, dynamic> resolved)
      : board = (resolved['content']['board'] as List)
            .map((t) => TileDef.fromJson(t as Map<String, dynamic>))
            .toList(),
        expropriation =
            resolved['content']['rules']['expropriation'] as int? ?? 0,
        rentBoost = resolved['content']['rules']['rent_boost'] as int? ?? 0,
        modIds = (resolved['mods'] as List)
            .map((m) => m['id'] as String)
            .toList();
}

/// Mirror of the engine `RuleParams` (ADR-0015). Absolute values; the host
/// edits them in the lobby and the server clamps.
class RuleParams {
  final int startingBalance;
  final int goSalary;
  final int jailFine;
  final int maxHousesPerProperty;
  final int bankruptcyThreshold;
  final bool auctionOnDecline;
  final int expropriation;
  final int rentBoost;
  final int winFullGroups;

  RuleParams.fromJson(Map<String, dynamic> j)
      : startingBalance = j['starting_balance'] as int,
        goSalary = j['go_salary'] as int,
        jailFine = j['jail_fine'] as int,
        maxHousesPerProperty = j['max_houses_per_property'] as int,
        bankruptcyThreshold = j['bankruptcy_threshold'] as int,
        auctionOnDecline = j['auction_on_decline'] as bool,
        expropriation = j['expropriation'] as int? ?? 0,
        rentBoost = j['rent_boost'] as int? ?? 0,
        winFullGroups = j['win_full_groups'] as int? ?? 0;
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

  PlayerView.fromJson(Map<String, dynamic> j)
      : id = j['id'] as String,
        name = j['name'] as String,
        cash = j['cash'] as int,
        position = j['position'] as int,
        inJail = j['in_jail'] as bool,
        jailCards = j['jail_cards'] as int? ?? 0,
        bankrupt = j['bankrupt'] as bool;
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

/// Flattened turn phase: `type` selects which of the nullable fields apply
/// (await_buy/auction carry a tile; auction carries the bid state).
class TurnPhase {
  final String type; // await_roll | await_buy | auction | await_end
  final int? tile;
  final int highBid;
  final int? highBidder;
  final int? turnSeat; // auction only: seat expected to bid or pass

  TurnPhase.fromJson(Map<String, dynamic> j)
      : type = j['type'] as String,
        tile = j['tile'] as int?,
        highBid = j['high_bid'] as int? ?? 0,
        highBidder = j['high_bidder'] as int?,
        turnSeat = j['turn'] as int?;
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

class ClientView {
  final bool finished;
  final int? winner;
  final List<PlayerView> players;
  final int current;
  final TurnPhase turn;
  final List<TileState> tiles;
  final List<TradeOffer> pendingTrades;

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
            .toList();
}

/// Human-readable line for one engine event (the animation/log feed).
/// Ported verbatim from the reference web client's `describe`.
String describeEvent(
  Map<String, dynamic> e,
  String Function(int seat) p,
  String Function(int tile) t,
) {
  switch (e['type']) {
    case 'turn_started':
      return "--- ${p(e['player'])}'s turn ---";
    case 'dice_rolled':
      return "${p(e['player'])} rolled ${e['d1']}+${e['d2']} = ${e['d1'] + e['d2']}";
    case 'moved':
      return "${p(e['player'])} moved to ${t(e['to'])}"
          "${e['passed_go'] == true ? ' (passed Go)' : ''}";
    case 'salary_paid':
      return "${p(e['player'])} collected \$${e['amount']} salary";
    case 'purchase_offered':
      return "${t(e['tile'])} is for sale: \$${e['price']}";
    case 'property_purchased':
      return "${p(e['player'])} bought ${t(e['tile'])} for \$${e['price']}";
    case 'purchase_declined':
      return "${p(e['player'])} declined ${t(e['tile'])}";
    case 'auction_started':
      return "Auction opened for ${t(e['tile'])}";
    case 'bid_placed':
      return "${p(e['player'])} bid \$${e['amount']}";
    case 'auction_passed':
      return "${p(e['player'])} passed";
    case 'auction_ended':
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
      return "${p(e['player'])} seized ${t(e['tile'])} from ${p(e['from'])} for \$${e['cost']}";
    case 'rent_boosted':
      return "${p(e['player'])} boosted ${t(e['tile'])} rent to level ${e['boosts']} for \$${e['cost']}";
    case 'property_mortgaged':
      return "${p(e['player'])} mortgaged ${t(e['tile'])} for \$${e['value']}";
    case 'property_unmortgaged':
      return "${p(e['player'])} redeemed ${t(e['tile'])} for \$${e['cost']}";
    case 'went_to_jail':
      return "${p(e['player'])} went to jail";
    case 'jail_fine_paid':
      return "${p(e['player'])} paid the \$${e['amount']} jail fine";
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
    default:
      return e.toString();
  }
}
