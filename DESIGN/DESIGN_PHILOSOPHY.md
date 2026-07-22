# Design philosophy

Everything else in this bible is derivable from this file. When two
rules conflict, come back here.

## The one-line identity

**A lamplit table between sharp friends.** Parcello should feel like a
beautiful physical object - dark wood, warm paper, a single brass lamp -
around which competitive people play FAST. Elegant but never precious;
competitive but never hostile; warm but never cute.

## Emotional goals (ranked - earlier wins conflicts)

1. **Clarity as respect.** The player's intelligence is the audience.
   Every pixel that makes the board harder to read is an insult; every
   one that makes a decision cheaper is hospitality. This is why
   "readability beats beauty" is motion law #1 and why information is
   GENEROUS by rule design (public hands, public cash, public forecast).
2. **Earned tension, honest release.** The game's drama comes from its
   real mechanics - a sealed bid, a springing trap, a doom clock - so
   the interface's job is to FRAME tension the rules created, never to
   manufacture fake tension (no artificial suspense pauses, no slot-
   machine reveals stretched for drama). Twelve seconds of a hairline
   draining on a lifted tile is real suspense; three seconds of confetti
   is noise.
3. **Composure.** The product never panics, begs, or celebrates harder
   than the moment deserves. Losing is rendered with dignity (a greyed
   pawn as a monument, not a mockery); winning is stillness and gold
   rules, not fireworks. Restraint is the luxury signal.
4. **Warmth.** Dark surfaces, warm off-whites, parchment. Competitive
   games drift cold and clinical; Parcello's counter-position is the
   materials of a physical game night.

## Target audience & positioning

People who want a REAL board-game evening online: friends first,
ranked-ladder strangers second, in 10-15 minute sessions. Positioning:
"the fast, elegant property game" - against Monopoly-likes (slower,
noisier, F2P-monetized) and against abstract ladder games (colder). The
Steam Deck sits in the couch-play centre of that vision; the 1024x600
floor and controller navigation are positioning decisions, not ports.

## Design values (operational form)

- **The board is the protagonist; the HUD is the receipt.** (Canonized
  in motion-language.md section 2.) Never build a feature whose primary
  surface is a feed when it could be the board.
- **One vocabulary, absorbed not taught.** Small alphabets (4 shapes,
  4 earcon categories, 3 attention devices) used with zero exceptions
  beat rich vocabularies used loosely. A player who has seen a gold
  chevron once knows what gold-in-motion means forever - IF nothing
  else ever moves in gold.
- **Truth over polish.** The interface never implies a state the server
  did not send, never animates a guess, never shows information the
  view masked (sealed bids stay sealed - see CONCEPT_CRITIQUE for how
  a beautiful mockup violated this).
- **Flat is a worldview.** No gradients, textures, glass, blur, or
  bounce - not as minimalism fashion, but because Art Deco is arrival,
  symmetry, and confident flatness; and because flat renders identically
  on a Deck, a browser canvas, and a 4K desktop.
- **Everything has a place, so nothing needs a search.** Banners appear
  in ONE place; rejections appear ON the thing that refused; the clock
  is ON the contested tile. Spatial consistency is the cheapest
  cognitive aid that exists.

## Non-goals (refuse these even when they would "help metrics")

- **No dark patterns, ever**: no energy, tickets, passes, FOMO timers,
  loot randomness, or pay-for-power (already dropped by decision in
  visual-identity.md; treat as permanent).
- **No casino register**: no coin fountains, no escalating jackpot
  numbers, no screen shake. Parcello is not a numbers-go-up game
  (motion-language section 11's Balatro note draws the exact line).
- **No mascot cuteness / cartoon register**: the warmth budget is spent
  on materials and light, not on a character.
- **No camera drama**: fixed camera always (competitive spatial memory
  is sacred).
- **No photorealism / skeuomorphic felt-and-wood**: the physical-table
  feeling comes from paper, ink, and brass ACCENTS, not from rendering
  a tabletop.

> **Reversal note (DDR-0023, owner 2026-07):** chat, a shop/currency, and
> levels/XP were categorical non-goals (formerly DDR-013); the owner
> reversed that toward the mockup's richer framing, and they now exist as
> inert visual placeholders (real later, each via its own ADR). This does
> NOT touch the first bullet: "no dark patterns / no pay-for-power" stays
> permanent, so any real shop must be cosmetic-only and any real progression
> non-coercive - the reversal opened the SURFACES, not the dark patterns.

## Long-term artistic vision (a decade out)

The end state this bible points at: the isometric "city of progress"
board (visual-identity.md) where buildings grow as flat, stepped Deco
silhouettes; a complete four-earcon audio identity plus a single
end-of-game musical resolution; haptics on Deck for the P1 arrest;
spectator and replay presentations that reuse the exact same grammar
(a replay is the director fed from a log - the architecture already
guarantees it). Growth direction: MORE staging of the same vocabulary,
never MORE vocabulary. If a future feature seems to need a new visual
language, the feature is probably fighting the game.
