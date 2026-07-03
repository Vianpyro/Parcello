//! Parcello game engine.
//!
//! Hard invariant (see architecture doc, section 4): this crate is pure and
//! synchronous. `Engine::apply` performs no I/O, spawns no tasks, and has no
//! side effects. Given the same `GameState` and `PlayerCommand`, it always
//! returns the same result. Randomness comes from a PRNG seed stored inside
//! `GameState`, which makes full games replayable from a command log.

pub mod command;
pub mod content;
pub mod error;
pub mod event;
pub mod rng;
pub mod state;
pub mod strategy;
pub mod view;

mod apply;

pub use command::{CommandKind, PlayerCommand};
pub use content::{
    CardDef, CardEffect, GameContent, PropertyDef, RentModel, RuleParams, TileDef, TileKind,
};
pub use error::{CommandError, ContentError};
pub use event::{DeckKind, Event};
pub use state::{GamePhase, GameState, Player, PlayerId, TileState, TradeOffer, TurnPhase};
pub use strategy::{BankruptcyResolver, DicePolicy, RentCalculator};
pub use view::ClientView;

use std::sync::Arc;

/// Authoritative rule executor for one room.
///
/// Built once at room creation from the resolved mod content; immutable for
/// the room's lifetime. Strategies are held behind `dyn` pointers so that V2
/// (WASM) can substitute implementations at room creation without touching
/// engine internals.
pub struct Engine {
    content: Arc<GameContent>,
    dice: Box<dyn DicePolicy>,
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
            dice: Box::new(strategy::UniformDice),
            rent: Box::new(strategy::StandardRent),
            bankruptcy: Box::new(strategy::StandardLiquidation),
        })
    }

    /// Injection points for alternative strategies (mods, tests).
    pub fn with_dice(mut self, dice: Box<dyn DicePolicy>) -> Self {
        self.dice = dice;
        self
    }

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
    /// `seed` drives every future random draw (dice, deck order). Two games
    /// with identical players, content, and seed are identical.
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
}

pub(crate) struct Strategies<'e> {
    pub dice: &'e dyn DicePolicy,
    pub rent: &'e dyn RentCalculator,
    pub bankruptcy: &'e dyn BankruptcyResolver,
}

impl Engine {
    pub(crate) fn strategies(&self) -> Strategies<'_> {
        Strategies {
            dice: self.dice.as_ref(),
            rent: self.rent.as_ref(),
            bankruptcy: self.bankruptcy.as_ref(),
        }
    }
}
