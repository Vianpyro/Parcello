//! Parcello game engine.
//!
//! Hard invariant (see architecture doc, section 4): this crate is pure and
//! synchronous. `Engine::apply` performs no I/O, spawns no tasks, and has no
//! side effects. Given the same `GameState` and `PlayerCommand`, it always
//! returns the same result. Randomness comes from a PRNG seed stored inside
//! `GameState`, which makes full games replayable from a command log.

pub mod bot;
pub mod command;
pub mod content;
pub mod error;
pub mod event;
pub mod rng;
pub mod state;
pub mod strategy;
pub mod view;

mod apply;
mod tuning;

pub use command::{CommandKind, PlayerCommand};
pub use content::{
    CardDef, CardEffect, GameContent, MarketEffect, MarketEventDef, PropertyDef, RentModel,
    RuleParams, TileDef, TileKind,
};
pub use error::{CommandError, ContentError};
pub use event::{DeckKind, Event};
pub use state::{
    ActiveMarketEvent, GamePhase, GameState, MarketForecast, Player, PlayerId, ScheduledEvent,
    Spotlight, TileState, TradeOffer, TurnPhase,
};
pub use strategy::{BankruptcyResolver, RentCalculator};
pub use view::{ClientView, PlayerView};

use std::sync::Arc;

/// Authoritative rule executor for one room.
///
/// Built once at room creation from the resolved mod content; immutable for
/// the room's lifetime. Strategies are held behind `dyn` pointers so that V2
/// (WASM) can substitute implementations at room creation without touching
/// engine internals.
pub struct Engine {
    content: Arc<GameContent>,
    rent: Box<dyn RentCalculator>,
    bankruptcy: Box<dyn BankruptcyResolver>,
}

impl Engine {
    /// Builds an engine with the default strategy implementations.
    /// Fails if the content violates board invariants (see `GameContent::validate`).
    pub fn new(content: Arc<GameContent>) -> Result<Self, ContentError> {
        content.validate()?;
        Ok(Self {
            content,
            rent: Box::new(strategy::StandardRent),
            bankruptcy: Box::new(strategy::StandardLiquidation),
        })
    }

    /// Injection point for an alternative strategy (mods, tests).
    pub fn with_rent(mut self, rent: Box<dyn RentCalculator>) -> Self {
        self.rent = rent;
        self
    }

    pub fn with_bankruptcy(mut self, resolver: Box<dyn BankruptcyResolver>) -> Self {
        self.bankruptcy = resolver;
        self
    }

    pub fn content(&self) -> &Arc<GameContent> {
        &self.content
    }

    /// Creates the initial state for a new game.
    ///
    /// `seed` drives every future random draw (deck order, market events).
    /// Two games with identical players, content, and seed are identical.
    pub fn new_game(&self, players: Vec<(PlayerId, String)>, seed: u64) -> GameState {
        GameState::new(&self.content, players, seed, &self.content.rules)
    }

    /// The single command pipeline: validate -> apply -> emit events.
    ///
    /// Invalid commands return `Err` and leave the caller's state untouched;
    /// the caller decides whether to log or forward the rejection. Accepted
    /// commands return the successor state plus the events describing what
    /// happened, in order.
    pub fn apply(
        &self,
        state: &GameState,
        cmd: &PlayerCommand,
    ) -> Result<(GameState, Vec<Event>), CommandError> {
        apply::apply(self, state, cmd)
    }

    /// Ends a time-boxed game: the richest surviving player (by net worth,
    /// ties to the lowest seat) wins. This is NOT a player command - the
    /// session layer calls it when the game clock expires. Pure and
    /// deterministic from the state, so a replay reconstructs it from the
    /// final Active state (ADR-0010). A no-op on an already-finished game.
    pub fn finish_on_time(&self, state: &GameState) -> (GameState, Vec<Event>) {
        if !matches!(state.phase, GamePhase::Active) {
            return (state.clone(), Vec::new());
        }
        // Strict `>` keeps the earlier seat on ties; `alive_players` yields
        // in seat order, so this is the lowest-seat tie-break.
        let winner = state
            .alive_players()
            .map(|p| (p, state.net_worth(&self.content, p)))
            .reduce(|best, cur| if cur.1 > best.1 { cur } else { best })
            .map_or(0, |(p, _)| p);
        let mut next = state.clone();
        next.phase = GamePhase::Finished { winner };
        (next, vec![Event::TimeUp { winner }])
    }
}

pub(crate) struct Strategies<'e> {
    pub rent: &'e dyn RentCalculator,
    pub bankruptcy: &'e dyn BankruptcyResolver,
}

impl Engine {
    pub(crate) fn strategies(&self) -> Strategies<'_> {
        Strategies {
            rent: self.rent.as_ref(),
            bankruptcy: self.bankruptcy.as_ref(),
        }
    }
}
