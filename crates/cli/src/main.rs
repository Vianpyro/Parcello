//! Terminal test client. Not a product surface: exists to exercise the
//! server end-to-end until the Flutter client lands.
//!
//! Commands (stdin): start | addbot | rmbot | set <field> <value> | roll | buy | no | bid <amount> | pass
//! | build <tile_id> | mortgage <tile_id> | redeem <tile_id>
//! | offer <seat> <give_cash> <give_tiles|-> <want_cash> <want_tiles|->
//! | accept <id> | refuse <id> | cancel <id> | pay | card | end | resign | quit.

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use parcello_engine::{ClientView, CommandKind, DeckKind, Event, GamePhase, TileKind, TurnPhase};
use parcello_mods::ResolvedContent;
use parcello_protocol::{AuthPayload, ClientMessage, RoomSettings, SeatInfo, ServerMessage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser, Debug)]
#[command(name = "parcello-cli", about = "Parcello terminal test client")]
struct Args {
    /// Server WebSocket URL.
    #[arg(long, default_value = "ws://127.0.0.1:7878/ws")]
    url: String,

    /// Guest display name (server must run with --insecure-guest).
    #[arg(long, required_unless_present = "token")]
    name: Option<String>,

    /// Identity token (EdDSA JWT from the identity provider, ADR-0009).
    #[arg(long)]
    token: Option<String>,

    /// Create a new room instead of joining one.
    #[arg(long, conflicts_with = "join")]
    create: bool,

    /// Ordered mod list for the created room (repeatable); omit for the
    /// server's default set.
    #[arg(long = "mod", requires = "create")]
    mods: Vec<String>,

    /// Join an existing room by code.
    #[arg(long)]
    join: Option<String>,

    /// Reconnect token from a previous join (printed on join; proves seat
    /// ownership when rejoining as a guest).
    #[arg(long)]
    reconnect: Option<String>,

    /// Autopilot: play with simple heuristics (solo playtesting). Stdin
    /// still works, e.g. to `start` the game from a bot host.
    #[arg(long)]
    bot: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.create && args.join.is_none() {
        return Err("pass --create or --join CODE".into());
    }

    let (socket, _) = tokio_tungstenite::connect_async(&args.url).await?;
    let (mut sink, mut stream) = socket.split();

    let auth = AuthPayload {
        token: args.token.clone(),
        // The server prefers the token when both are present.
        guest_name: args.name.clone(),
        reconnect: args.reconnect.clone(),
    };
    let first = if args.create {
        ClientMessage::Create {
            auth,
            mods: (!args.mods.is_empty()).then(|| args.mods.clone()),
        }
    } else {
        ClientMessage::Join {
            code: args.join.clone().expect("checked above"),
            auth,
        }
    };
    sink.send(Message::Text(serde_json::to_string(&first)?.into()))
        .await?;

    println!(
        "connected to {} as {}",
        args.url,
        args.name.as_deref().unwrap_or("(identity token)")
    );
    println!(
        "commands: start | roll | buy | no | bid <n> | pass | build <t> | sell <t> | seize <t> (landing tile only, end of turn) | boost <t> | mortgage <t> | redeem <t> | pay | card | end | resign | quit"
    );
    println!(
        "trading:  offer <seat> <give$> <give_tiles|-> <want$> <want_tiles|->  (tiles comma-separated)"
    );
    println!("          accept <id> | refuse <id> | cancel <id>");
    println!("post-game: feedback <1-5> [comment]");

    let mut ctx = Ctx::default();
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(frame) = frame else { break };
                match frame? {
                    Message::Text(text) => {
                        let msg: ServerMessage = serde_json::from_str(&text)?;
                        // Only view-bearing messages can require a new bot
                        // decision; reacting to `Rejected` would loop.
                        let fresh_view = matches!(
                            &msg,
                            ServerMessage::GameStarted { .. }
                                | ServerMessage::Update { .. }
                                | ServerMessage::Joined { view: Some(_), .. }
                        );
                        ctx.render(msg);
                        if args.bot && fresh_view
                            && let (Some(content), Some(view), Some(me)) =
                                (&ctx.content, &ctx.view, ctx.my_seat)
                            && let Some(kind) =
                                parcello_engine::bot::decide(&content.content, view, me)
                        {
                            // Pace bot actions so a watching client has time
                            // to play out the pawn-movement animation.
                            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                            let cmd = ClientMessage::Cmd { cmd: kind };
                            sink.send(Message::Text(serde_json::to_string(&cmd)?.into()))
                                .await?;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            line = stdin.next_line() => {
                let Some(line) = line? else { break };
                let Some(msg) = parse_command(&ctx, line.trim()) else {
                    if line.trim() == "quit" { break; }
                    if !line.trim().is_empty() {
                        println!("? unknown command: {line}");
                    }
                    continue;
                };
                sink.send(Message::Text(serde_json::to_string(&msg)?.into()))
                    .await?;
            }
        }
    }

    let _ = sink.close().await;
    println!("disconnected");
    Ok(())
}

fn parse_command(ctx: &Ctx, line: &str) -> Option<ClientMessage> {
    let mut parts = line.split_whitespace();
    let cmd = match (parts.next()?, parts.next()) {
        ("start", None) => return Some(ClientMessage::Start),
        ("again", None) => return Some(ClientMessage::PlayAgain),
        ("leave", None) => return Some(ClientMessage::Leave),
        ("addbot", None) => return Some(ClientMessage::AddBot),
        ("rmbot", None) => return Some(ClientMessage::RemoveBot),
        ("set", Some(field)) => {
            let value = parts.next()?;
            let mut settings = ctx.settings.clone()?;
            apply_setting(&mut settings, field, value)?;
            return Some(ClientMessage::Configure { settings });
        }
        ("roll", None) => CommandKind::Roll,
        ("buy", None) => CommandKind::Buy,
        ("no", None) => CommandKind::Decline,
        ("bid", Some(n)) => CommandKind::Bid {
            amount: n.parse().ok()?,
        },
        ("pass", None) => CommandKind::Pass,
        ("build", Some(tile)) => CommandKind::Build {
            tile: tile.to_string(),
        },
        ("sell", Some(tile)) => CommandKind::SellHouse {
            tile: tile.to_string(),
        },
        ("seize", Some(tile)) => CommandKind::Expropriate {
            tile: tile.to_string(),
        },
        ("boost", Some(tile)) => CommandKind::BoostRent {
            tile: tile.to_string(),
        },
        ("mortgage", Some(tile)) => CommandKind::Mortgage {
            tile: tile.to_string(),
        },
        ("redeem", Some(tile)) => CommandKind::Unmortgage {
            tile: tile.to_string(),
        },
        ("offer", Some(seat)) => {
            let to = ctx.ids.get(seat.parse::<usize>().ok()?)?.clone();
            let give_cash = parts.next()?.parse().ok()?;
            let give_tiles = parse_tile_list(parts.next()?);
            let receive_cash = parts.next()?.parse().ok()?;
            let receive_tiles = parse_tile_list(parts.next()?);
            CommandKind::ProposeTrade {
                to,
                give_cash,
                give_tiles,
                receive_cash,
                receive_tiles,
            }
        }
        ("accept", Some(id)) => CommandKind::AcceptTrade {
            trade: id.parse().ok()?,
        },
        ("refuse", Some(id)) => CommandKind::DeclineTrade {
            trade: id.parse().ok()?,
        },
        ("cancel", Some(id)) => CommandKind::CancelTrade {
            trade: id.parse().ok()?,
        },
        ("feedback", Some(rating)) => {
            let rest: Vec<&str> = parts.collect();
            return Some(ClientMessage::Feedback {
                rating: rating.parse().ok()?,
                comment: (!rest.is_empty()).then(|| rest.join(" ")),
            });
        }
        ("pay", None) => CommandKind::PayJailFine,
        ("card", None) => CommandKind::UseJailCard,
        ("end", None) => CommandKind::EndTurn,
        ("resign", None) => CommandKind::Resign,
        _ => return None,
    };
    Some(ClientMessage::Cmd { cmd })
}

/// "-" means an empty side; otherwise comma-separated tile ids.
fn parse_tile_list(raw: &str) -> Vec<String> {
    if raw == "-" {
        return Vec::new();
    }
    raw.split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Applies one `set <field> <value>` edit to a settings copy (ADR-0015).
/// `None` on an unknown field or unparseable value. The server clamps.
fn apply_setting(s: &mut RoomSettings, field: &str, value: &str) -> Option<()> {
    // `off`/`none` disables a timer; anything else is a seconds count.
    let opt_secs = |v: &str| -> Option<Option<u64>> {
        if v == "off" || v == "none" {
            Some(None)
        } else {
            Some(Some(v.parse().ok()?))
        }
    };
    let r = &mut s.rules;
    match field {
        "game" => s.game_seconds = opt_secs(value)?,
        "turn" => s.turn_seconds = opt_secs(value)?,
        "bank" => s.time_bank_seconds = opt_secs(value)?,
        "starting_balance" => r.starting_balance = value.parse().ok()?,
        "go_salary" => r.go_salary = value.parse().ok()?,
        "jail_fine" => r.jail_fine = value.parse().ok()?,
        "max_houses" => r.max_houses_per_property = value.parse().ok()?,
        "bankruptcy_threshold" => r.bankruptcy_threshold = value.parse().ok()?,
        "auction" => r.auction_on_decline = value.parse().ok()?,
        "expropriation" => r.expropriation = value.parse().ok()?,
        "rent_boost" => r.rent_boost = value.parse().ok()?,
        "win_full_groups" => r.win_full_groups = value.parse().ok()?,
        "subsidiary_pool" => r.subsidiary_pool_factor = value.parse().ok()?,
        "conglomerate_pool" => r.conglomerate_pool_factor = value.parse().ok()?,
        _ => return None,
    }
    Some(())
}

/// Everything needed to print human-readable output: resolved content for
/// tile names, latest seat list for player names, our own seat index.
#[derive(Default)]
struct Ctx {
    content: Option<ResolvedContent>,
    names: Vec<String>,
    /// Stable player ids by seat, for addressing trade offers.
    ids: Vec<String>,
    my_seat: Option<usize>,
    /// Latest authoritative view; what the bot decides on.
    view: Option<Box<ClientView>>,
    /// Latest room settings (ADR-0015); the `set` command edits a copy.
    settings: Option<RoomSettings>,
}

impl Ctx {
    fn render(&mut self, msg: ServerMessage) {
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
            "* settings: game={} turn={} bank={} | starting_balance={} go_salary={} jail_fine={} \
             max_houses={} bankruptcy_threshold={} auction_on_decline={} expropriation={} \
             rent_boost={} win_full_groups={} subsidiary_pool={} conglomerate_pool={}",
            secs(s.game_seconds),
            secs(s.turn_seconds),
            secs(s.time_bank_seconds),
            r.starting_balance,
            r.go_salary,
            r.jail_fine,
            r.max_houses_per_property,
            r.bankruptcy_threshold,
            r.auction_on_decline,
            r.expropriation,
            r.rent_boost,
            r.win_full_groups,
            r.subsidiary_pool_factor,
            r.conglomerate_pool_factor,
        );
    }

    fn print_view(&mut self, view: &ClientView) {
        self.names = view.players.iter().map(|p| p.name.clone()).collect();
        self.ids = view.players.iter().map(|p| p.id.clone()).collect();
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
            println!(
                "{marker}{me} {} ${} @ {}{}",
                p.name,
                p.cash,
                self.tile_name(p.position),
                status
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
            GamePhase::Active => {
                let (actor, hint) = match &view.turn {
                    TurnPhase::AwaitRoll => (view.current, "roll".to_string()),
                    TurnPhase::AwaitBuy { tile } => (
                        view.current,
                        format!("buy | no ({})", self.tile_name(*tile)),
                    ),
                    TurnPhase::AwaitEnd => (
                        view.current,
                        "end (or build/seize <tile_id> on the tile you're standing on)".to_string(),
                    ),
                    TurnPhase::Auction {
                        tile,
                        high_bid,
                        high_bidder,
                        turn,
                        ..
                    } => {
                        let high = match high_bidder {
                            Some(b) => format!("${high_bid} by {}", self.player(*b)),
                            None => "no bids".to_string(),
                        };
                        (
                            *turn,
                            format!("AUCTION {} ({high}): bid <n> | pass", self.tile_name(*tile)),
                        )
                    }
                };
                println!("  -> {} to act: {hint}", self.player(actor));
            }
        }
    }

    fn describe(&self, event: &Event) -> String {
        match event {
            Event::TurnStarted { player } => format!("--- {}'s turn ---", self.player(*player)),
            Event::DiceRolled { player, d1, d2 } => {
                format!("{} rolled {d1}+{d2} = {}", self.player(*player), d1 + d2)
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
            Event::PurchaseOffered { tile, price, .. } => {
                format!("{} is for sale: ${price}", self.tile_name(*tile))
            }
            Event::PropertyPurchased {
                player,
                tile,
                price,
            } => {
                format!(
                    "{} bought {} for ${price}",
                    self.player(*player),
                    self.tile_name(*tile)
                )
            }
            Event::PurchaseDeclined { player, tile } => {
                format!(
                    "{} declined {}",
                    self.player(*player),
                    self.tile_name(*tile)
                )
            }
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
            Event::AuctionStarted { tile } => {
                format!("auction opened for {}", self.tile_name(*tile))
            }
            Event::BidPlaced { player, amount, .. } => {
                format!("{} bid ${amount}", self.player(*player))
            }
            Event::AuctionPassed { player, .. } => format!("{} passed", self.player(*player)),
            Event::AuctionEnded {
                tile,
                winner,
                amount,
            } => match winner {
                Some(w) => format!(
                    "{} won the auction for {} at ${amount}",
                    self.player(*w),
                    self.tile_name(*tile)
                ),
                None => format!("{} stays unsold", self.tile_name(*tile)),
            },
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
            Event::WentToJail { player } => format!("{} went to jail", self.player(*player)),
            Event::JailFinePaid { player, amount } => {
                format!("{} paid the ${amount} jail fine", self.player(*player))
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
