//! Terminal rendering: the `Ctx` projection of server messages, the
//! per-event `describe` lines, and the table printout. Split from
//! `main.rs` for module size (2026-07).

use parcello_engine::{ClientView, DeckKind, Event, GamePhase, TileKind, TurnPhase};
use parcello_mods::ResolvedContent;
use parcello_protocol::{RoomSettings, SeatInfo, ServerMessage};

/// Everything needed to print human-readable output: resolved content for
/// tile names, latest seat list for player names, our own seat index.
#[derive(Default)]
pub(crate) struct Ctx {
    pub(crate) content: Option<ResolvedContent>,
    names: Vec<String>,
    /// Stable player ids by seat, for addressing trade offers.
    pub(crate) ids: Vec<String>,
    pub(crate) my_seat: Option<usize>,
    /// Latest authoritative view; what the bot decides on.
    pub(crate) view: Option<Box<ClientView>>,
    /// Latest room settings (ADR-0015); the `set` command edits a copy.
    pub(crate) settings: Option<RoomSettings>,
}

impl Ctx {
    pub(crate) fn render(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::RoomCreated { code } => {
                println!("* room created: {code} (share this code)");
            }
            ServerMessage::Joined {
                code,
                seat,
                players,
                content,
                view,
                reconnect,
                time_remaining,
                turn_seconds,
                time_bank_seconds,
                settings,
            } => {
                self.my_seat = Some(seat);
                self.content = Some(*content);
                self.settings = Some(settings);
                println!("* joined room {code} as seat {seat}");
                if let Some(token) = reconnect {
                    println!("* reconnect token: {token} (rejoin with --reconnect {token})");
                }
                if let Some(secs) = time_remaining {
                    println!("* timed game: {secs}s left (richest wins)");
                }
                if let Some(secs) = turn_seconds {
                    println!("* turn timer: {secs}s per turn");
                }
                if let Some(secs) = time_bank_seconds {
                    println!("* time bank: {secs}s personal reserve, never refilled");
                }
                self.print_lobby(&players);
                self.print_settings();
                if let Some(view) = view {
                    println!("* game in progress:");
                    self.print_view(&view);
                    self.view = Some(view);
                }
            }
            ServerMessage::Lobby { players, settings } => {
                // Keep effective rules current for the --bot heuristic.
                if let Some(content) = &mut self.content {
                    content.content.rules = settings.rules.clone();
                }
                self.settings = Some(settings);
                self.print_lobby(&players);
                self.print_settings();
            }
            ServerMessage::GameStarted {
                view,
                time_remaining,
                turn_seconds,
                time_bank_seconds,
            } => {
                println!("* game started");
                if let Some(secs) = time_remaining {
                    println!("* timed game: {secs}s (richest wins)");
                }
                if let Some(secs) = turn_seconds {
                    println!("* turn timer: {secs}s per turn");
                }
                if let Some(secs) = time_bank_seconds {
                    println!("* time bank: {secs}s personal reserve, never refilled");
                }
                self.print_view(&view);
                self.view = Some(view);
            }
            ServerMessage::Update {
                events,
                view,
                banks,
                ..
            } => {
                for event in &events {
                    println!("  {}", self.describe(event));
                }
                if let Some(banks) = banks
                    && let Some(seat) = self.my_seat
                    && let Some(&remaining) = banks.get(seat)
                {
                    println!("  (time bank: {remaining}s left)");
                }
                self.print_view(&view);
                self.view = Some(view);
            }
            ServerMessage::Rejected { error } => println!("! rejected: {error}"),
            ServerMessage::Error { message } => println!("! error: {message}"),
            ServerMessage::Pong => println!("* pong"),
        }
    }

    fn print_lobby(&mut self, players: &[SeatInfo]) {
        self.ids = players.iter().map(|p| p.player_id.clone()).collect();
        let list: Vec<String> = players
            .iter()
            .map(|p| {
                let status = if p.is_bot {
                    " (bot)"
                } else if p.connected {
                    ""
                } else {
                    " (offline)"
                };
                format!("{}:{}{}", p.seat, p.name, status)
            })
            .collect();
        println!("* lobby: [{}]", list.join(", "));
    }

    /// Dumps the current room settings; the field names double as `set` keys.
    fn print_settings(&self) {
        let Some(s) = &self.settings else { return };
        let r = &s.rules;
        let secs = |v: Option<u64>| v.map_or("off".to_string(), |n| format!("{n}s"));
        println!(
            "* settings: game={} turn={} bank={} | starting_balance={} go_salary={} \
             velocity_min={} velocity_max={} \
             max_houses={} bankruptcy_threshold={} expropriation={} \
             rent_boost={} win_full_groups={} win_victory_points={} subsidiary_pool={} conglomerate_pool={}",
            secs(s.game_seconds),
            secs(s.turn_seconds),
            secs(s.time_bank_seconds),
            r.starting_balance,
            r.go_salary,
            r.velocity_min,
            r.velocity_max,
            r.max_houses_per_property,
            r.bankruptcy_threshold,
            r.expropriation,
            r.rent_boost,
            r.win_full_groups,
            r.win_victory_points,
            r.subsidiary_pool_factor,
            r.conglomerate_pool_factor,
        );
    }

    fn print_view(&mut self, view: &ClientView) {
        self.names = view.players.iter().map(|p| p.name.clone()).collect();
        self.ids = view.players.iter().map(|p| p.id.clone()).collect();
        // Victory-point race (ADR-0020): "the race IS the game" when on,
        // so show progress toward the target next to every player.
        let vp_target = self
            .content
            .as_ref()
            .map_or(0, |c| c.content.rules.win_victory_points);
        for (i, p) in view.players.iter().enumerate() {
            let marker = if i == view.current { ">" } else { " " };
            let me = if Some(i) == self.my_seat { "*" } else { " " };
            let status = if p.bankrupt {
                " BANKRUPT".to_string()
            } else if p.in_jail {
                " [jail]".to_string()
            } else {
                String::new()
            };
            let vp = if vp_target > 0 {
                format!(" VP:{}/{vp_target}", p.victory_points)
            } else {
                String::new()
            };
            println!(
                "{marker}{me} {} ${} @ {}{status}{vp}",
                p.name,
                p.cash,
                self.tile_name(p.position),
            );
        }
        if view.subsidiaries_available.is_some() || view.conglomerates_available.is_some() {
            let pool = |p: Option<u64>| p.map_or("unlimited".to_string(), |n| n.to_string());
            println!(
                "  pools: subsidiaries={} conglomerates={}",
                pool(view.subsidiaries_available),
                pool(view.conglomerates_available)
            );
        }
        if let Some(active) = &view.forecast.active {
            println!(
                "  market: {} active ({:+}%, ends turn {})",
                self.market_event_name(&active.event_id),
                active.magnitude_pct,
                active.ends_at_turn
            );
        }
        if !view.forecast.queue.is_empty() {
            let upcoming: Vec<String> = view
                .forecast
                .queue
                .iter()
                .map(|s| {
                    format!(
                        "{} (turn {})",
                        self.market_event_name(&s.event_id),
                        s.starts_at_turn
                    )
                })
                .collect();
            println!("  forecast: {}", upcoming.join(", "));
        }
        for t in &view.pending_trades {
            println!(
                "  trade #{}: {} gives {} for {} (to {}: accept {id} | refuse {id})",
                t.id,
                self.player(t.from),
                self.trade_side(t.give_cash, &t.give_tiles),
                self.trade_side(t.receive_cash, &t.receive_tiles),
                self.player(t.to),
                id = t.id,
            );
        }
        match view.phase {
            GamePhase::Finished { winner } => {
                println!("=== game over, winner: {} ===", self.player(winner));
            }
            GamePhase::Active => match &view.turn {
                TurnPhase::AwaitMove => {
                    let acting = self.player(view.current);
                    let player = &view.players[view.current];
                    if let Some(route) = &player.jail_route {
                        println!(
                            "  -> {acting} to act: play {} (locked route, {} left: {route:?})",
                            route[0],
                            route.len()
                        );
                    } else if player.in_jail {
                        println!(
                            "  -> {acting} to act: route <permutation of hand, comma-separated> | bribe <amount> | card"
                        );
                    } else {
                        println!("  -> {acting} to act: play <n> (hand: {:?})", player.hand);
                    }
                }
                // Every living opponent may vote at once (ADR-0024), not a
                // single actor: list who's still pending, and prompt only
                // when it's our own seat still waiting.
                TurnPhase::BribeVote {
                    briber,
                    amount,
                    votes,
                } => {
                    let pending: Vec<String> = votes
                        .iter()
                        .enumerate()
                        .filter(|&(i, v)| i != *briber && v.is_none())
                        .map(|(i, _)| self.player(i))
                        .collect();
                    let waiting_on_me = self.my_seat.is_some_and(|seat| {
                        seat != *briber && votes.get(seat).is_some_and(|v| v.is_none())
                    });
                    let hint = if waiting_on_me { ": vote yes|no" } else { "" };
                    println!(
                        "  -> {} offers a ${amount} bribe (5s window), waiting on: {}{hint}",
                        self.player(*briber),
                        pending.join(", ")
                    );
                }
                TurnPhase::AwaitEnd => {
                    println!(
                        "  -> {} to act: end (or build/seize <tile_id> on the tile you're standing on)",
                        self.player(view.current)
                    );
                }
                // Every living seat may bid at once (ADR-0018), not a
                // single actor: list who's still pending, and prompt only
                // when it's our own seat still waiting.
                TurnPhase::BlindAuction { tile, bids } => {
                    let pending: Vec<String> = bids
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| b.is_none())
                        .map(|(i, _)| self.player(i))
                        .collect();
                    let waiting_on_me = self
                        .my_seat
                        .is_some_and(|seat| bids.get(seat).is_some_and(|b| b.is_none()));
                    let hint = if waiting_on_me {
                        ": bid <n> (0 abstains)"
                    } else {
                        ""
                    };
                    println!(
                        "  -> sealed bid open on {} (5s window), waiting on: {}{hint}",
                        self.tile_name(*tile),
                        pending.join(", ")
                    );
                }
            },
        }
    }

    fn describe(&self, event: &Event) -> String {
        match event {
            Event::TurnStarted { player } => format!("--- {}'s turn ---", self.player(*player)),
            Event::MovementCardPlayed { player, value } => {
                format!("{} played movement card {value}", self.player(*player))
            }
            Event::Moved {
                player,
                to,
                passed_go,
                ..
            } => {
                let go = if *passed_go { " (passed Go)" } else { "" };
                format!(
                    "{} moved to {}{go}",
                    self.player(*player),
                    self.tile_name(*to)
                )
            }
            Event::SalaryPaid { player, amount } => {
                format!("{} collected ${amount} salary", self.player(*player))
            }
            Event::BlindAuctionOpened {
                tile,
                discoverer,
                floor,
            } => format!(
                "{} landed on {}: sealed bid open (${floor} floor for {})",
                self.player(*discoverer),
                self.tile_name(*tile),
                self.player(*discoverer),
            ),
            Event::BlindBidSubmitted { player } => {
                format!("{} submitted a bid", self.player(*player))
            }
            Event::BlindAuctionResolved {
                tile,
                winner,
                amount,
                ..
            } => match winner {
                Some(w) => format!(
                    "{} won {} at ${amount}",
                    self.player(*w),
                    self.tile_name(*tile)
                ),
                None => format!("{} stays unsold", self.tile_name(*tile)),
            },
            Event::TradeProposed { trade, from, to } => format!(
                "{} proposed trade #{trade} to {}",
                self.player(*from),
                self.player(*to)
            ),
            Event::TradeAccepted { trade, from, to } => format!(
                "{} accepted trade #{trade} from {}",
                self.player(*to),
                self.player(*from)
            ),
            Event::TradeDeclined { trade, .. } => format!("trade #{trade} declined"),
            Event::TradeCancelled { trade, .. } => format!("trade #{trade} cancelled"),
            Event::RentPaid {
                from,
                to,
                tile,
                amount,
            } => format!(
                "{} paid ${amount} rent to {} for {}",
                self.player(*from),
                self.player(*to),
                self.tile_name(*tile)
            ),
            Event::TaxPaid { player, amount, .. } => {
                format!("{} paid ${amount} tax", self.player(*player))
            }
            Event::CardDrawn {
                player, deck, text, ..
            } => {
                let deck = match deck {
                    DeckKind::Chance => "chance",
                    DeckKind::Community => "community",
                };
                format!("{} drew a {deck} card: {text}", self.player(*player))
            }
            Event::CashAdjusted {
                player,
                delta,
                reason,
            } => {
                let verb = if *delta >= 0 { "received" } else { "paid" };
                format!(
                    "{} {verb} ${} ({reason})",
                    self.player(*player),
                    delta.abs()
                )
            }
            Event::HouseBuilt {
                player,
                tile,
                houses,
                cost,
            } => format!(
                "{} built on {} (now {houses}) for ${cost}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::HouseSold {
                player,
                tile,
                houses,
                refund,
            } => format!(
                "{} sold a house on {} (now {houses}) for ${refund}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::Expropriated {
                player,
                from,
                tile,
                cost,
                liquidated,
                liquidation_refund,
            } => {
                let base = format!(
                    "{} seized {} from {} for ${cost}",
                    self.player(*player),
                    self.tile_name(*tile),
                    self.player(*from)
                );
                if *liquidated > 0 {
                    format!(
                        "{base} ({liquidated} levels liquidated, ${liquidation_refund} to the former owner)"
                    )
                } else {
                    base
                }
            }
            Event::RentBoosted {
                player,
                tile,
                boosts,
                cost,
            } => format!(
                "{} boosted {} rent to level {boosts} for ${cost}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::RentBoostConsumed { tile } => format!(
                "the boost on {} is spent (one-shot trap)",
                self.tile_name(*tile)
            ),
            Event::RoundBonusAwarded { player, points } => format!(
                "{} is the round's cash leader: +{points} permanent VP",
                self.player(*player)
            ),
            Event::PropertyMortgaged {
                player,
                tile,
                value,
            } => format!(
                "{} mortgaged {} for ${value}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::PropertyUnmortgaged { player, tile, cost } => format!(
                "{} redeemed {} for ${cost}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::WentToJail { player, .. } => {
                format!("{} went to jail", self.player(*player))
            }
            Event::LegalRouteChosen { player, order } => format!(
                "{} chose a Legal Route {order:?} (rent-free on their tiles until it's done)",
                self.player(*player)
            ),
            Event::BribeOffered { player, amount } => format!(
                "{} offers a ${amount} bribe to leave jail",
                self.player(*player)
            ),
            Event::BribeVoteCast { player } => {
                format!("{} voted on the bribe", self.player(*player))
            }
            Event::BribeResolved {
                briber,
                amount,
                succeeded,
                accepts,
                total,
            } => {
                if *succeeded {
                    format!(
                        "bribe accepted ({accepts}/{total}): {} pays ${amount}, split among the table",
                        self.player(*briber)
                    )
                } else {
                    format!(
                        "bribe rejected ({accepts}/{total}): {} stays in jail",
                        self.player(*briber)
                    )
                }
            }
            Event::JailCardReceived { player } => format!(
                "{} received a get-out-of-jail-free card",
                self.player(*player)
            ),
            Event::JailCardUsed { player } => {
                format!("{} used a get-out-of-jail-free card", self.player(*player))
            }
            Event::LeftJail { player } => format!("{} left jail", self.player(*player)),
            Event::PropertyTransferred { tile, from, to } => match to {
                Some(to) => format!(
                    "{} transferred to {} (from {})",
                    self.tile_name(*tile),
                    self.player(*to),
                    self.player(*from)
                ),
                None => format!("{} returned to the bank", self.tile_name(*tile)),
            },
            Event::PlayerBankrupt { player, creditor } => match creditor {
                Some(c) => format!(
                    "{} went bankrupt (creditor: {})",
                    self.player(*player),
                    self.player(*c)
                ),
                None => format!("{} went bankrupt", self.player(*player)),
            },
            Event::PlayerResigned { player } => format!("{} resigned", self.player(*player)),
            Event::GameEnded { winner } => format!("game ended, {} wins", self.player(*winner)),
            Event::TimeUp { winner } => {
                format!("time's up! {} wins on net worth", self.player(*winner))
            }
            Event::WonByGroups { winner, groups } => format!(
                "{} wins by controlling {groups} colour groups!",
                self.player(*winner)
            ),
            Event::WonByPoints { player, points } => {
                format!(
                    "{} wins with {points} victory points!",
                    self.player(*player)
                )
            }
            Event::WonByPoolExhaustion { winner } => format!(
                "the conglomerate pool ran dry - {} wins on victory points!",
                self.player(*winner)
            ),
            Event::MarketEventActivated {
                event_id,
                magnitude_pct,
                duration_turns,
                ..
            } => {
                let name = self.market_event_name(event_id);
                if *duration_turns == 0 {
                    format!("market event: {name} ({magnitude_pct:+}%)")
                } else {
                    format!("market event: {name} ({magnitude_pct:+}% for {duration_turns} turns)")
                }
            }
            Event::MarketEventExpired { event_id } => {
                format!("market event ended: {}", self.market_event_name(event_id))
            }
            Event::SpotlightStarted {
                tile,
                rent_pct,
                duration_turns,
            } => {
                let span = if *duration_turns <= 0 {
                    "until the next Exposition landing".to_string()
                } else {
                    format!("for {duration_turns} turns")
                };
                format!(
                    "The Exposition spotlights {} (+{rent_pct}% rent {span})",
                    self.tile_name(*tile)
                )
            }
            Event::SpotlightEnded { tile } => {
                format!("the spotlight on {} fades", self.tile_name(*tile))
            }
        }
    }

    fn trade_side(&self, cash: i64, tiles: &[usize]) -> String {
        let tiles: Vec<String> = tiles.iter().map(|&t| self.tile_name(t)).collect();
        match (cash, tiles.is_empty()) {
            (0, true) => "nothing".to_string(),
            (0, false) => tiles.join(" + "),
            (c, true) => format!("${c}"),
            (c, false) => format!("${c} + {}", tiles.join(" + ")),
        }
    }

    fn player(&self, seat: usize) -> String {
        self.names
            .get(seat)
            .cloned()
            .unwrap_or_else(|| format!("player {seat}"))
    }

    fn tile_name(&self, index: usize) -> String {
        let Some(content) = &self.content else {
            return format!("tile {index}");
        };
        match content.content.board.get(index) {
            Some(tile) => {
                if let TileKind::Property(_) = tile.kind {
                    format!("{} [{}]", tile.name, tile.id)
                } else {
                    tile.name.clone()
                }
            }
            None => format!("tile {index}"),
        }
    }

    fn market_event_name(&self, event_id: &str) -> String {
        self.content
            .as_ref()
            .and_then(|c| c.content.market_event(event_id))
            .map_or_else(|| event_id.to_string(), |def| def.name.clone())
    }
}
