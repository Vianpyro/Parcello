//! Bots showcase supervisor (ADR-0035): keeps one all-bot game running as a
//! last resort, so a `spectate` with no code always finds something to
//! watch on an otherwise idle server.
//!
//! Same shape as the ranked matchmaker (ADR-0034): a single Tokio task
//! owned by the server, ticking over the room registry. It only ever
//! *creates* the showcase; winding one down is the room's own idle timeout
//! (no connected seats, no spectators) - so a game someone is still
//! watching is never yanked away, and one stops being recreated the moment
//! humans have an active game of their own.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::AppState;
use crate::room::{RoomCmd, RoomProbe, create_showcase_room};

/// Supervisor cadence: how often the registry is scanned. Slow on purpose -
/// the showcase is a fallback, not a service level.
const TICK: Duration = Duration::from_secs(15);

/// Boots the showcase supervisor. A no-op unless `--showcase` is on.
pub fn spawn_showcase(app: AppState) {
    if !app.showcase {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if needs_showcase(&app).await {
                match create_showcase_room(
                    &app.rooms,
                    Arc::clone(&app.content),
                    Arc::clone(&app.history),
                    app.turn_timeout,
                    app.time_bank,
                    app.game_timeout,
                )
                .await
                {
                    Ok(code) => info!(room = %code, "bots showcase started (ADR-0035)"),
                    Err(e) => warn!(error = %e, "showcase room creation failed"),
                }
            }
        }
    });
}

/// True when no room has an Active game with a connected human AND no
/// showcase room exists (a finished-but-alive showcase counts: it replays
/// itself, creating a second would double the fan-out for nothing).
async fn needs_showcase(app: &AppState) -> bool {
    let handles: Vec<mpsc::Sender<RoomCmd>> = app.rooms.read().await.values().cloned().collect();
    let mut humans_playing = false;
    let mut showcase_exists = false;
    for handle in handles {
        let (reply, on_reply) = oneshot::channel();
        if handle.send(RoomCmd::Probe { reply }).await.is_err() {
            continue;
        }
        let Ok(probe) = on_reply.await else { continue };
        let RoomProbe {
            active,
            humans,
            showcase,
        } = probe;
        humans_playing |= active && humans > 0;
        showcase_exists |= showcase;
    }
    !humans_playing && !showcase_exists
}
