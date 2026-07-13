//! Terminal test client. Not a product surface: exists to exercise the
//! server end-to-end until the Flutter client lands.
//!
//! Stdin commands (`<n>` values, `|` alternatives):
//!
//! ```text
//! start | addbot | rmbot | set <field> <value>
//! play <n> (movement card) | route <n,n,...> (Legal Route, a full
//!   permutation of the hand) | bribe <amount> | vote yes|no (5s window,
//!   ADR-0024) | card (jail card) | bid <amount> (0 abstains; landing on
//!   an unowned tile opens a sealed-bid window for every living seat,
//!   ADR-0018)
//! build <tile_id> | mortgage <tile_id> | redeem <tile_id>
//! offer <seat> <give_cash> <give_tiles|-> <want_cash> <want_tiles|->
//! accept <id> | refuse <id> | cancel <id> | end | resign | quit
//! ```

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use parcello_protocol::{AuthPayload, ClientMessage, ServerMessage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

/// Pause before each autopilot move so a watching human client has time
/// to play out the pawn-movement animation (ADR-0028's beats run ~1.5s
/// for a typical move).
const BOT_PACE: std::time::Duration = std::time::Duration::from_millis(1500);

#[derive(Parser, Debug)]
#[command(name = "parcello-cli", about = "Parcello terminal test client")]
struct Args {
    /// Server WebSocket URL.
    #[arg(long, default_value = "ws://127.0.0.1:7878/ws")]
    url: String,

    /// Guest display name (server must run with --insecure-guest).
    #[arg(long, required_unless_present = "token")]
    name: Option<String>,

    /// Identity token (`EdDSA` JWT from the identity provider, ADR-0009).
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
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    anyhow::ensure!(
        args.create || args.join.is_some(),
        "pass --create or --join CODE"
    );

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
        "commands: start | play <n> | route <n,n,...> | bribe <amount> | vote yes|no | card | bid <n> (0 abstains) | build <t> | sell <t> | seize <t> (landing tile only, end of turn) | boost <t> | mortgage <t> | redeem <t> | end | resign | quit"
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
                        // Terminal output is instant: ack every Update
                        // immediately (ADR-0028) so this client never gates
                        // anyone's timers - the built-in exerciser of the
                        // "I don't animate" path.
                        let ack_seq = match &msg {
                            ServerMessage::Update { seq, .. } => Some(*seq),
                            _ => None,
                        };
                        ctx.render(msg);
                        if let Some(seq) = ack_seq {
                            let ack = ClientMessage::AnimationDone { through_seq: seq };
                            sink.send(Message::Text(serde_json::to_string(&ack)?.into()))
                                .await?;
                        }
                        // Bid-jitter noise for the shared heuristic: clock
                        // nanos are plenty for a test client (no new dep).
                        let noise = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| u64::from(d.subsec_nanos()) ^ d.as_secs());
                        if args.bot && fresh_view
                            && let (Some(content), Some(view), Some(me)) =
                                (&ctx.content, &ctx.view, ctx.my_seat)
                            && let Some(kind) =
                                parcello_engine::bot::decide(&content.content, view, me, noise)
                        {
                            tokio::time::sleep(BOT_PACE).await;
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

mod input;
mod ui;

use input::parse_command;
use ui::Ctx;
