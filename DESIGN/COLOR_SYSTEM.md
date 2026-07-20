# Color system - semantics

Hex values live in `docs/visual-identity.md` and `lib/tokens.dart`
(single sources). This file defines what each colour MEANS, when it
must be used, and when it must not. Meaning is the asset: a palette
this small only works because every colour is a word.

## The semantic table

| Semantic | Token(s) | MUST be used for | MUST NOT be used for |
|---|---|---|---|
| **Value / subject / time** | `pc-gold` (+`pc-gold-dark` pressed) | primary CTAs; currency AMOUNT text; the attention hairline (subject frames, focus rings, window drains); corner tiles; VP (the ONLY thing allowed to move in gold) | body text; large fills; decorative flourish; anything that moves and is not VP |
| **Gain / positive** | `pc-sage` (`pc-gainInk` on parchment) | money arriving at YOU; group completed; positive market tint; the centre plaza | generic "success" chrome (a completed settings save is not a gain); text |
| **Loss / threat / danger** | `pc-oxblood` (`pc-lossInk` on parchment) | money leaving YOU; attacks (takeover, trap, tax); destructive buttons (Resign); error text; time-bank draining; negative market tint | mere emphasis; warnings that are not threats; headers |
| **Economy / paper** | `pc-parchment` + `pc-parchment-ink` | the play objects: tile faces, chits, movement cards, receipts, offer cards | interface chrome (panels, dialogs, menus stay dark) |
| **Ownership / identity** | pawn colours | pawn, seat marker, tile band once owned, bid-reveal tags | ANY fixed UI meaning (pawn colours are per-seat variables, never semantics) |
| **Property group** | the 8 group colours | tile edge bands, group indicators | UI states of any kind |
| **Information / chrome text** | `pc-text` / `-muted` / `-faint` | primary / secondary / whisper text ("unranked", captions, ghost values) | conveying game meaning by shade alone |
| **Structure** | `pc-border`, `pc-border-muted` | hairlines, dividers; dashed = "not yet available" | emphasis |
| **Selection / focus** | `pc-gold` ring (1-2 px, sharp) | keyboard/controller focus, selected chip, the acting seat | multi-select states that could confuse with "subject" during a window |
| **Ranking / premium** | `pc-gold` accents on `pc-surface-2` | rating display, winner staging, conglomerate caps | tier ladders with new hues (tiers, if ever, get a DDR and stay in-palette) |

## The three colour laws

1. **No third valence.** There is deliberately NO amber/warning colour.
   Everything is gain (sage), loss/threat (oxblood), or neutral. A
   "warning" is expressed as a threat (oxblood) if it costs you, or as
   information (muted text) if it doesn't - forcing that decision keeps
   the message honest. Adding a yellow warning would also collide with
   gold's reserved meanings. (Recorded as DDR-004.)
2. **Gold-in-motion means victory points.** Statically gold is value/
   subject/time; the moment gold MOVES it is VP and nothing else. This
   is the players' learned grammar - one violation and it unlearns.
3. **Colour is never the only channel.** Every valence is also a
   direction (rises/falls), a sign (+/-), and a position (source ->
   target). ~8% of players cannot rely on red-green; the grammar
   already doesn't require them to (ACCESSIBILITY.md).

## States (uniform across components)

- Hover: surface step up (`surface` -> `surface-2`) + hover earcon.
- Pressed: `pc-gold-dark` on gold elements; surface unchanged elsewhere.
- Disabled: keep shape, drop to `pc-text-faint` on `pc-surface`; never
  hide the element if the action exists but is unavailable (the greyed
  Connect button teaches WHY via the caption beside it).
- Focus: the gold ring, only on actionable elements (C-series
  invariant: board tiles are focusable only when actionable).
- Error/refusal: the SUBJECT flashes/holds oxblood; never a red page.

## Adding a colour

You don't. A genuinely new need (say, a colour-blind alternate set, or
the isometric board's window-light accents) is a DDR with a validated
palette extension in visual-identity.md + tokens.dart, checked for
contrast (ACCESSIBILITY) and for collision with the three laws.
