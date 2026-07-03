//! Terminal test client. Not a product surface: exists to exercise the
//! server end-to-end until the Flutter client lands.
//!
//! Commands (stdin): start | roll | buy | no | bid <amount> | pass
//! | build <tile_id> | mortgage <tile_id> | redeem <tile_id>
//! | offer <seat> <give_cash> <give_tiles|-> <want_cash> <want_tiles|->
//! | accept <id> | refuse <id> | cancel <id> | pay | end | resign | quit.

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use parcello_engine::{ClientView, CommandKind, DeckKind, Event, GamePhase, TileKind, TurnPhase};
use parcello_mods::ResolvedContent;
use parcello_protocol::{AuthPayload, ClientMessage, SeatInfo, ServerMessage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser, Debug)]
#[command(name = "parcello-cli", about = "Parcello terminal test client")]
struct Args {
    /// Server WebSocket URL.
    #[arg(long, default_value = "ws://127.0.0.1:7878/ws")]
    url: String,

    /// Guest display name (server must run with --insecure-guest).
    #[arg(long)]
    name: String,

    /// Create a new room instead of joining one.
    #[arg(long, conflicts_with = "join")]
    create: bool,

    /// Join an existing room by code.
    #[arg(long)]
    join: Option<String>,
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
        token: None,
        guest_name: Some(args.name.clone()),
    };
    let first = if args.create {
        ClientMessage::Create { auth }
    } else {
        ClientMessage::Join {
            code: args.join.clone().expect("checked above"),
            auth,
        }
    };
    sink.send(Message::Text(serde_json::to_string(&first)?)).await?;

    println!("connected to {} as {}", args.url, args.name);
    println!(
        "commands: start | roll | buy | no | bid <n> | pass | build <t> | sell <t> | mortgage <t> | redeem <t> | pay | end | resign | quit"
    );
    println!(
        "trading:  offer <seat> <give$> <give_tiles|-> <want$> <want_tiles|->  (tiles comma-separated)"
    );
    println!("          accept <id> | refuse <id> | cancel <id>");

    let mut ctx = Ctx::default();
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(frame) = frame else { break };
                match frame? {
                    Message::Text(text) => {
                        let msg: ServerMessage = serde_json::from_str(&text)?;
                        ctx.render(msg);
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
                sink.send(Message::Text(serde_json::to_string(&msg)?)).await?;
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
        ("roll", None) => CommandKind::Roll,
        ("buy", None) => CommandKind::Buy,
        ("no", None) => CommandKind::Decline,
        ("bid", Some(n)) => CommandKind::Bid { amount: n.parse().ok()? },
        ("pass", None) => CommandKind::Pass,
        ("build", Some(tile)) => CommandKind::Build { tile: tile.to_string() },
        ("sell", Some(tile)) => CommandKind::SellHouse { tile: tile.to_string() },
        ("mortgage", Some(tile)) => CommandKind::Mortgage { tile: tile.to_string() },
        ("redeem", Some(tile)) => CommandKind::Unmortgage { tile: tile.to_string() },
        ("offer", Some(seat)) => {
            let to = ctx.ids.get(seat.parse::<usize>().ok()?)?.clone();
            let give_cash = parts.next()?.parse().ok()?;
            let give_tiles = parse_tile_list(parts.next()?);
            let receive_cash = parts.next()?.parse().ok()?;
            let receive_tiles = parse_tile_list(parts.next()?);
            CommandKind::ProposeTrade { to, give_cash, give_tiles, receive_cash, receive_tiles }
        }
        ("accept", Some(id)) => CommandKind::AcceptTrade { trade: id.parse().ok()? },
        ("refuse", Some(id)) => CommandKind::DeclineTrade { trade: id.parse().ok()? },
        ("cancel", Some(id)) => CommandKind::CancelTrade { trade: id.parse().ok()? },
        ("pay", None) => CommandKind::PayJailFine,
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
    raw.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()
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
}

impl Ctx {
    fn render(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::RoomCreated { code } => {
                println!("* room created: {code} (share this code)");
            }
            ServerMessage::Joined { code, seat, players, content, view } => {
                self.my_seat = Some(seat);
                self.content = Some(*content);
                println!("* joined room {code} as seat {seat}");
                self.print_lobby(&players);
                if let Some(view) = view {
                    println!("* game in progress:");
                    self.print_view(&view);
                }
            }
            ServerMessage::Lobby { players } => self.print_lobby(&players),
            ServerMessage::GameStarted { view } => {
                println!("* game started");
                self.print_view(&view);
            }
            ServerMessage::Update { events, view } => {
                for event in &events {
                    println!("  {}", self.describe(event));
                }
                self.print_view(&view);
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
                let status = if p.connected { "" } else { " (offline)" };
                format!("{}:{}{}", p.seat, p.name, status)
            })
            .collect();
        println!("* lobby: [{}]", list.join(", "));
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
                    TurnPhase::AwaitBuy { tile } => {
                        (view.current, format!("buy | no ({})", self.tile_name(*tile)))
                    }
                    TurnPhase::AwaitEnd => {
                        (view.current, "end (or build <tile_id>)".to_string())
                    }
                    TurnPhase::Auction { tile, high_bid, high_bidder, turn, .. } => {
                        let high = match high_bidder {
                            Some(b) => format!("${high_bid} by {}", self.player(*b)),
                            None => "no bids".to_string(),
                        };
                        (*turn, format!("AUCTION {} ({high}): bid <n> | pass", self.tile_name(*tile)))
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
            Event::Moved { player, to, passed_go, .. } => {
                let go = if *passed_go { " (passed Go)" } else { "" };
                format!("{} moved to {}{go}", self.player(*player), self.tile_name(*to))
            }
            Event::SalaryPaid { player, amount } => {
                format!("{} collected ${amount} salary", self.player(*player))
            }
            Event::PurchaseOffered { tile, price, .. } => {
                format!("{} is for sale: ${price}", self.tile_name(*tile))
            }
            Event::PropertyPurchased { player, tile, price } => {
                format!("{} bought {} for ${price}", self.player(*player), self.tile_name(*tile))
            }
            Event::PurchaseDeclined { player, tile } => {
                format!("{} declined {}", self.player(*player), self.tile_name(*tile))
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
            Event::TradeDeclined { trade } => format!("trade #{trade} declined"),
            Event::TradeCancelled { trade } => format!("trade #{trade} cancelled"),
            Event::AuctionStarted { tile } => {
                format!("auction opened for {}", self.tile_name(*tile))
            }
            Event::BidPlaced { player, amount, .. } => {
                format!("{} bid ${amount}", self.player(*player))
            }
            Event::AuctionPassed { player, .. } => format!("{} passed", self.player(*player)),
            Event::AuctionEnded { tile, winner, amount } => match winner {
                Some(w) => format!(
                    "{} won the auction for {} at ${amount}",
                    self.player(*w),
                    self.tile_name(*tile)
                ),
                None => format!("{} stays unsold", self.tile_name(*tile)),
            },
            Event::RentPaid { from, to, tile, amount } => format!(
                "{} paid ${amount} rent to {} for {}",
                self.player(*from),
                self.player(*to),
                self.tile_name(*tile)
            ),
            Event::TaxPaid { player, amount, .. } => {
                format!("{} paid ${amount} tax", self.player(*player))
            }
            Event::CardDrawn { player, deck, text, .. } => {
                let deck = match deck {
                    DeckKind::Chance => "chance",
                    DeckKind::Community => "community",
                };
                format!("{} drew a {deck} card: {text}", self.player(*player))
            }
            Event::CashAdjusted { player, delta, reason } => {
                let verb = if *delta >= 0 { "received" } else { "paid" };
                format!("{} {verb} ${} ({reason})", self.player(*player), delta.abs())
            }
            Event::HouseBuilt { player, tile, houses, cost } => format!(
                "{} built on {} (now {houses}) for ${cost}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::HouseSold { player, tile, houses, refund } => format!(
                "{} sold a house on {} (now {houses}) for ${refund}",
                self.player(*player),
                self.tile_name(*tile)
            ),
            Event::PropertyMortgaged { player, tile, value } => format!(
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
}
