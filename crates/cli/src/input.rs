//! Stdin command parsing: one line in, one `ClientMessage` out (or None
//! for an unknown/blank line). Split from `main.rs` for module size
//! (2026-07); the prompt strings live in `main.rs` next to the loop.

use parcello_engine::CommandKind;
use parcello_protocol::{ClientMessage, RoomSettings};

use crate::ui::Ctx;

pub(crate) fn parse_command(ctx: &Ctx, line: &str) -> Option<ClientMessage> {
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
        ("play", Some(n)) => CommandKind::PlayMovementCard {
            value: n.parse().ok()?,
        },
        ("route", Some(list)) => CommandKind::ChooseLegalRoute {
            order: parse_u8_list(list),
        },
        ("bribe", Some(n)) => CommandKind::OfferBribe {
            amount: n.parse().ok()?,
        },
        ("vote", Some(v)) => CommandKind::VoteOnBribe {
            accept: match v {
                "yes" => true,
                "no" => false,
                _ => return None,
            },
        },
        ("bid", Some(n)) => CommandKind::SubmitBlindBid {
            amount: n.parse().ok()?,
        },
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

/// Comma-separated card values for `route <n,n,...>` (ADR-0024); unparseable
/// entries are dropped rather than rejecting the whole line - the server
/// validates the permutation and rejects it cleanly if it's wrong.
fn parse_u8_list(raw: &str) -> Vec<u8> {
    raw.split(',').filter_map(|s| s.parse().ok()).collect()
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
        "velocity_min" => r.velocity_min = value.parse().ok()?,
        "velocity_max" => r.velocity_max = value.parse().ok()?,
        "max_houses" => r.max_houses_per_property = value.parse().ok()?,
        "bankruptcy_threshold" => r.bankruptcy_threshold = value.parse().ok()?,
        "expropriation" => r.expropriation = value.parse().ok()?,
        "rent_boost" => r.rent_boost = value.parse().ok()?,
        "win_full_groups" => r.win_full_groups = value.parse().ok()?,
        "win_points" => r.win_victory_points = value.parse().ok()?,
        "subsidiary_pool" => r.subsidiary_pool_factor = value.parse().ok()?,
        "conglomerate_pool" => r.conglomerate_pool_factor = value.parse().ok()?,
        _ => return None,
    }
    Some(())
}
