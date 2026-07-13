//! Money movement and partial-payment bankruptcy: every debt in the
//! game settles through `charge`.
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::{Event, Exec};

impl Exec<'_> {
    /// Moves `amount` from `debtor` to `creditor` (`None` = bank). Triggers
    /// liquidation, then bankruptcy, when cash cannot stay above the
    /// configured threshold. Semantic events (rent, tax, ...) are emitted by
    /// callers; this only emits distress events.
    pub(super) fn charge(&mut self, debtor: usize, creditor: Option<usize>, amount: i64) {
        if amount <= 0 {
            return;
        }
        let threshold = self.content.rules.bankruptcy_threshold;
        let needed = amount + threshold;
        if self.st.players[debtor].cash < needed {
            self.strategies.bankruptcy.liquidate(
                self.content,
                &mut self.st,
                debtor,
                needed,
                &mut self.ev,
            );
        }
        if self.st.players[debtor].cash >= needed {
            self.st.players[debtor].cash -= amount;
            if let Some(c) = creditor {
                self.st.players[c].cash += amount;
            }
            return;
        }
        // Partial settlement: the creditor receives whatever cash remains.
        let remaining = self.st.players[debtor].cash.max(0);
        self.st.players[debtor].cash -= remaining;
        if let Some(c) = creditor {
            self.st.players[c].cash += remaining;
        }
        self.bankrupt(debtor, creditor);
    }

    pub(super) fn bankrupt(&mut self, p: usize, creditor: Option<usize>) {
        self.st.pending_trades.retain(|t| t.from != p && t.to != p);
        let cap = self.content.rules.max_houses_per_property.min(5);
        for tile in 0..self.st.tiles.len() {
            if self.st.tiles[tile].owner == Some(p) {
                // Bank refurbishes (no compensation), but the shared pools
                // still get their units back (ADR-0019) - a pure release.
                self.st.release_tile_pools(self.st.tiles[tile].houses, cap);
                self.st.tiles[tile].owner = creditor;
                self.st.tiles[tile].houses = 0;
                self.st.tiles[tile].boosts = 0;
                if creditor.is_none() {
                    // Returned to the bank: sold clean next time.
                    self.st.tiles[tile].mortgaged = false;
                }
                self.ev.push(Event::PropertyTransferred {
                    tile,
                    from: p,
                    to: creditor,
                });
            }
        }
        let player = &mut self.st.players[p];
        player.bankrupt = true;
        player.jailed = false;
        player.jail_route = None;
        player.jail_cards = 0;
        self.ev.push(Event::PlayerBankrupt {
            player: p,
            creditor,
        });
        self.check_win();
    }
}
