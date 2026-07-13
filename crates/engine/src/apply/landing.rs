//! Landing resolution: rent with its multiplier chain (boost ->
//! forecast -> spotlight), card draws and chains, taxes, and the
//! Exposition spotlight (ADR-0026).
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::{
    CardEffect, DeckKind, Event, Exec, GamePhase, MAX_CARD_CHAIN_DEPTH, MarketEffect,
    RENT_BOOST_STEP_PCT, SPOTLIGHT_NO_EXPIRY, Spotlight, TileKind, TurnPhase,
};

impl Exec<'_> {
    /// Applies a tile's rent-boost level to a base rent (ADR-0012):
    /// `+RENT_BOOST_STEP_PCT%` per boost.
    pub(super) fn boosted_rent(base: i64, boosts: u8) -> i64 {
        base * (100 + RENT_BOOST_STEP_PCT * i64::from(boosts)) / 100
    }

    /// Applies the active market event's magnitude to `base` if it matches
    /// `effect` (ADR-0021); a no-op otherwise, including while nothing is
    /// active. Shared by rent (`resolve_landing`) and takeover cost
    /// (`expropriate`).
    pub(super) fn apply_market_multiplier(&self, effect: MarketEffect, base: i64) -> i64 {
        match &self.st.forecast.active {
            Some(active) if active.effect == effect => {
                (base * (100 + active.magnitude_pct) / 100).max(0)
            }
            _ => base,
        }
    }

    /// Applies the active spotlight's bonus to `base` if it targets `tile`
    /// (ADR-0026); a no-op otherwise, including while nothing is spotlit.
    /// Composes with `boosted_rent` and `apply_market_multiplier` as the
    /// third multiplicative step.
    pub(super) fn apply_spotlight_multiplier(&self, tile: usize, base: i64) -> i64 {
        match &self.st.spotlight {
            Some(sp) if sp.tile == tile => {
                (base * (100 + self.content.rules.spotlight_rent_pct) / 100).max(0)
            }
            _ => base,
        }
    }

    /// Landing on the Exposition corner (ADR-0026): draws a random property
    /// via the seeded RNG and puts it in the spotlight, bumping whatever was
    /// previously spotlit. A no-op (no event, no state change) when the
    /// board has no property tiles at all.
    ///
    /// `spotlight_duration_turns <= 0` means the spotlight is permanent -
    /// only the next Exposition landing replaces it (2026-07 playtest
    /// decision; the mechanic's off switch is `spotlight_rent_pct = 0`, or
    /// simply not placing a `Spotlight` tile).
    pub(super) fn enter_spotlight(&mut self) {
        let Some(tile) = self.st.draw_spotlight_tile(self.content) else {
            return;
        };
        if let Some(old) = self.st.spotlight.take() {
            self.ev.push(Event::SpotlightEnded { tile: old.tile });
        }
        let duration = self.content.rules.spotlight_duration_turns;
        let expires_at_turn = if duration <= 0 {
            SPOTLIGHT_NO_EXPIRY
        } else {
            self.st.turn_count + duration as u32
        };
        self.st.spotlight = Some(Spotlight {
            tile,
            expires_at_turn,
        });
        self.ev.push(Event::SpotlightStarted {
            tile,
            rent_pct: self.content.rules.spotlight_rent_pct,
            duration_turns: duration,
        });
    }

    /// Turn-transition tick: expires the spotlight once its window closes.
    /// Unlike `tick_forecast` there is no queue to refill - a new spotlight
    /// only ever starts from a fresh landing on the corner.
    pub(super) fn tick_spotlight(&mut self) {
        if let Some(sp) = &self.st.spotlight
            && self.st.turn_count >= sp.expires_at_turn
        {
            let tile = sp.tile;
            self.st.spotlight = None;
            self.ev.push(Event::SpotlightEnded { tile });
        }
    }

    pub(super) fn resolve_landing(&mut self, p: usize, depth: u8) {
        if depth > MAX_CARD_CHAIN_DEPTH {
            self.st.turn = TurnPhase::AwaitEnd;
            return;
        }
        let tile = self.st.players[p].position;
        match &self.content.board[tile].kind {
            TileKind::Go | TileKind::Jail | TileKind::FreeParking => {
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::Spotlight => {
                self.enter_spotlight();
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::GoToJail => {
                self.go_to_jail(p);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::Tax { amount } => {
                let amount = *amount;
                self.ev.push(Event::TaxPaid {
                    player: p,
                    tile,
                    amount,
                });
                self.charge(p, None, amount);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::NetWorthTax { min_pct, max_pct } => {
                // Progressive audit (ADR-0029): a seeded-random bracket of
                // the lander's CURRENT net worth - punishes hoarding
                // proportionally, and the weighted draw keeps the brutal
                // brackets rare.
                let (min_pct, max_pct) = (*min_pct, *max_pct);
                let pct = self.st.draw_networth_tax_pct(min_pct, max_pct);
                let amount = self.st.net_worth(self.content, p) * i64::from(pct) / 100;
                self.ev.push(Event::TaxPaid {
                    player: p,
                    tile,
                    amount,
                });
                self.charge(p, None, amount);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::Property(prop) => match self.st.tiles[tile].owner {
                None => {
                    self.ev.push(Event::BlindAuctionOpened {
                        tile,
                        discoverer: p,
                        floor: prop.price,
                    });
                    self.st.turn = TurnPhase::BlindAuction {
                        tile,
                        bids: vec![None; self.st.players.len()],
                    };
                }
                Some(owner) if owner == p => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(_) if self.st.tiles[tile].mortgaged => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                // Legal Route rent freeze (ADR-0024): visitors play free
                // on this owner's tiles for as long as their route lasts.
                Some(owner) if self.st.players[owner].jail_route.is_some() => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(owner) => {
                    let base = self.strategies.rent.rent(self.content, &self.st, tile);
                    let rent = Self::boosted_rent(base, self.st.tiles[tile].boosts);
                    let rent = self.apply_market_multiplier(MarketEffect::RentMultiplier, rent);
                    let rent = self.apply_spotlight_multiplier(tile, rent);
                    self.ev.push(Event::RentPaid {
                        from: p,
                        to: owner,
                        tile,
                        amount: rent,
                    });
                    // A boost is a one-shot trap (ADR-0012, amended
                    // 2026-07): the first rent collected at the boosted
                    // rate consumes the whole boost, whatever its level.
                    // Cleared before `charge` on purpose - the trap is
                    // sprung by the landing, even if the payer then goes
                    // through partial-payment bankruptcy.
                    if self.st.tiles[tile].boosts > 0 {
                        self.st.tiles[tile].boosts = 0;
                        self.ev.push(Event::RentBoostConsumed { tile });
                    }
                    self.charge(p, Some(owner), rent);
                    self.st.turn = TurnPhase::AwaitEnd;
                }
            },
            TileKind::Chance => self.draw_card(p, DeckKind::Chance, depth),
            TileKind::Community => self.draw_card(p, DeckKind::Community, depth),
        }
    }

    pub(super) fn draw_card(&mut self, p: usize, deck: DeckKind, depth: u8) {
        let idx = match deck {
            DeckKind::Chance => self.st.chance_deck.draw(),
            DeckKind::Community => self.st.community_deck.draw(),
        };
        let Some(idx) = idx else {
            // Validated content never hits this; mod-broken decks degrade to a no-op.
            self.st.turn = TurnPhase::AwaitEnd;
            return;
        };
        let card = match deck {
            DeckKind::Chance => self.content.chance[idx].clone(),
            DeckKind::Community => self.content.community[idx].clone(),
        };
        self.ev.push(Event::CardDrawn {
            player: p,
            deck,
            card: card.id.clone(),
            text: card.text.clone(),
        });
        self.apply_card_effect(p, &card.id, &card.effect, depth);
    }

    pub(super) fn apply_card_effect(
        &mut self,
        p: usize,
        card_id: &str,
        effect: &CardEffect,
        depth: u8,
    ) {
        match effect {
            CardEffect::Money { amount } => {
                if *amount >= 0 {
                    self.st.players[p].cash += amount;
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: *amount,
                        reason: card_id.to_string(),
                    });
                } else {
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: *amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(p, None, -amount);
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::MoveTo { tile, collect_go } => {
                let to = self
                    .content
                    .tile_index(tile)
                    .expect("validated content: card targets exist");
                self.teleport(p, to, *collect_go);
                self.resolve_landing(p, depth + 1);
            }
            CardEffect::MoveBy { steps } => {
                if *steps >= 0 {
                    self.move_forward(p, *steps as usize);
                } else {
                    let len = self.content.board.len() as i64;
                    let from = self.st.players[p].position as i64;
                    let to = (from + i64::from(*steps)).rem_euclid(len) as usize;
                    self.teleport(p, to, false);
                }
                self.resolve_landing(p, depth + 1);
            }
            CardEffect::GoToJail => {
                self.go_to_jail(p);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::GetOutOfJail => {
                self.st.players[p].jail_cards += 1;
                self.ev.push(Event::JailCardReceived { player: p });
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::CollectFromEach { amount } => {
                let others: Vec<usize> = self.st.alive_players().filter(|&o| o != p).collect();
                for o in others {
                    self.ev.push(Event::CashAdjusted {
                        player: o,
                        delta: -amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(o, Some(p), *amount);
                    if matches!(self.st.phase, GamePhase::Finished { .. }) {
                        return;
                    }
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::PayEach { amount } => {
                let others: Vec<usize> = self.st.alive_players().filter(|&o| o != p).collect();
                for o in others {
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: -amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(p, Some(o), *amount);
                    if self.st.players[p].bankrupt
                        || matches!(self.st.phase, GamePhase::Finished { .. })
                    {
                        return;
                    }
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
        }
    }
}
