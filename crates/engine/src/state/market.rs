//! The public market layer of the state: the rolling forecast queue
//! (ADR-0021) and the Exposition spotlight (ADR-0026). Both are fully
//! public knowledge - the drama is everyone seeing the same storm coming.

use serde::{Deserialize, Serialize};

use crate::content::{GameContent, MarketEffect};
use crate::rng;

/// A drawn-but-not-yet-active market event (ADR-0021): public the moment
/// it's scheduled, so players can plan around it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub event_id: String,
    pub starts_at_turn: u32,
    pub duration: u32,
}

/// The market event currently in effect, if any (ADR-0021). Only
/// `RentMultiplier`/`AcquisitionMultiplier` ever occupy this - `WealthTax`
/// resolves instantly the moment it activates and never lingers here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveMarketEvent {
    pub event_id: String,
    pub effect: MarketEffect,
    pub magnitude_pct: i64,
    pub ends_at_turn: u32,
}

/// Public market forecast queue (ADR-0021): the next scheduled events plus
/// whichever one is currently in effect. Empty and permanently inert when
/// the content's `market_events` pool is empty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MarketForecast {
    /// Upcoming events, oldest (soonest) first, kept at 3 entries.
    pub queue: Vec<ScheduledEvent>,
    pub active: Option<ActiveMarketEvent>,
}

/// The property currently in the Exposition corner's spotlight (ADR-0026):
/// its rent is boosted until `expires_at_turn`.
///
/// Public in `ClientView`
/// unconditionally - the whole point is that the table sees the hot tile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Spotlight {
    pub tile: usize,
    pub expires_at_turn: u32,
}

impl MarketForecast {
    /// Draws one event from `content.market_events` and schedules it after
    /// whatever is already queued (or after `now` if the queue is empty),
    /// `content.forecast_gap_turns` later. A complete no-op - no RNG draw -
    /// when the pool is empty, so mods without `events.toml` never perturb
    /// the seeded RNG stream. Used both to seed the initial 3 events and to
    /// refill the queue each time one activates.
    pub(crate) fn draw_next(&mut self, content: &GameContent, rng: &mut u64, now: u32) {
        if content.market_events.is_empty() {
            return;
        }
        let idx = rng::below(rng, content.market_events.len() as u64) as usize;
        let def = &content.market_events[idx];
        let after = self.queue.last().map_or(now, |s| s.starts_at_turn);
        self.queue.push(ScheduledEvent {
            event_id: def.id.clone(),
            starts_at_turn: after + content.forecast_gap_turns,
            duration: def.duration_turns,
        });
    }
}
