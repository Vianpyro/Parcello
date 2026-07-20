# Commercial UX audit

The honest inventory of what stands between the current build and a
commercial-quality experience, prioritized. This is the DESIGN twin of
`docs/roadmap-and-product.md` (which owns the product/feature roadmap
and version phases) and `docs/technical-debt.md` (code debt) - here the
lens is purely the player's felt experience. Overlaps are cross-
referenced, not restated.

Grounding: the real-build screenshots (connect, menu, lobby/board,
rules) confirm the FOUNDATION reads correctly at real sizes - dark
register, gold accents, framed panels, legible board. The gaps below
are polish and completeness, not foundation. The single largest
authored gap is already named in motion-language section 13 ("built vs
not built"); this audit widens the lens to the whole product.

Priority: **Critical** (blocks feeling like a finished product) /
**High** (strong felt-quality lever) / **Medium** / **Low**. Impact =
effect on a real player's session.

## Critical

| Gap | Impact | Notes |
|---|---|---|
| Real multi-human playtest, then tune | Everything downstream is a bet until this happens | Owns the whole roadmap; the UI decisions here await its verdict (roadmap-and-product) |
| Sealed-bid input not anchored to the tile; clock still a corner number | The GAME'S CORE LOOP lacks its designed moment; auctions feel like a form, not a decision | motion-language 8.2 + 13; DESIGN_SYSTEM auction widget. #1 authored gap. (Note: the tile-lift/recede half IS built - the gap is the input + clock) |
| Web OIDC sign-in never verified against a real IdP | On guest-off servers this is the ONLY way in; if it's broken, the door doesn't open | technical-debt D2 |

## High

| Gap | Impact | Notes |
|---|---|---|
| Audio: 4 earcons silent, dice stand-in on card play | The game is half-mute; sound is huge feel-per-effort | AUDIO_DIRECTION; sfx README; motion-language 14 |
| AFK auto-played marker missing | "The server played my turn and nothing told me" - a trust break | motion-language 8.4; a persistent marker, not a toast |
| Ranked UI (queue screen, MMR on player card) | Ranked exists but is CLI-only; no felt progression | roadmap Beta; PLAYER_EXPERIENCE rank |
| Match history surfaced to players | No sense of a journey between sessions | Data exists in `--history`; needs a read path (ADR-0005) |
| Trade animations (still log-only) | Negotiation - a social pillar - has no felt moment | motion-language 13 |
| Reconnect re-orientation (snaps to truth, doesn't re-orient) | Returning players are disoriented | motion-language 9 + 13 |
| Screen-reader support absent | Blocks players who need it; blocks some markets/launch bars | ACCESSIBILITY; the log is the tractable seed |

## Medium

| Gap | Impact | Notes |
|---|---|---|
| Bribe vote reveals as banner, not votes flipping face-up | The second simultaneous-decision moment lacks its payoff | motion-language 13 |
| Contested-win rebate not shown on the chit in flight | The discoverer's edge is invisible mid-motion | `BidReveal.discounted` computed, unused; motion-language 13 |
| Time-bank-draining P2 alarm; bot-thinking pulse; hand-refill beat | Silent state changes read as hangs or go unnoticed | motion-language 8.4 |
| Spotlight expiring by turn count just vanishes | A world change with no exit motion | motion-language 8.4 |
| Spectate-by-code in the menu (only server-pick today) | Watching a specific friend's game needs the CLI | Wire+CLI support exists (ADR-0035) |
| CVD (colour-blind) palette audit + alternate set | ~8% of players; group-colour distinguishability unverified | ACCESSIBILITY; DDR when acted on |
| Empty/loading polish (queue heartbeat copy, richer empty states) | Waits feel dead without "what's being awaited" | UX_GUIDELINES; SCREEN_ARCHITECTURE |

## Low

| Gap | Impact | Notes |
|---|---|---|
| Isometric board (flat today) | The aspirational look; big chantier | visual-identity.md; ART_DIRECTION; motion primitives already survive projection |
| Haptics on Steam Deck (P1 + threat) | Couch-play delight | motion-language 14 |
| High-contrast profile | Accessibility completeness | ACCESSIBILITY |
| Relaxed (longer-timer) preset | Accessibility + casual audience | DDR-015; configurability already exists (untimed rooms) |
| Achievements (per-server, non-intrusive) | Retention nod; must never interrupt play | guide #12; GAME_FEEL |
| Replay viewer | Learning + shareable moments | Format already replays (ADR-0001); a director data-source swap |
| Player profile / journal surface | Between-session identity | PLAYER_EXPERIENCE; narrative not report-card |

## How to use this audit

Do not work top-to-bottom mechanically: the Critical playtest gate
reorders everything after it (features built before it are bets). Once
playtests speak, the High tier is the felt-quality sprint - audio, the
auction anchor, rent-to-the-earner, and the AFK marker are the four
that most change how a session FEELS for the least code. Every item
here, when built, is checked against DESIGN_REVIEW before it ships.
