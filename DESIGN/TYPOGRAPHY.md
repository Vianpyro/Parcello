# Typography

Families and roles are canonized in `docs/visual-identity.md` (Fraunces
display / Inter UI / Source Serif 4 tile labels; all OFL, bundled
offline in `assets/fonts/`). This file is the usage system.

## Roles (never mix them)

| Family | Voice | Where | Never |
|---|---|---|---|
| **Fraunces 700** | the brand speaking | wordmark, end-screen titles, rules-page headings | body text, buttons, anything at small sizes |
| **Inter 400/500/700** | the machine speaking | ALL functional text: buttons, labels, amounts, timers, logs, dialogs | decorative display |
| **Source Serif 4** | the world speaking | property names on tiles/cards (paper voice) | UI chrome |

The three-voice split is doing register work: brand (rare, ceremonial),
machine (constant, neutral), world (paper objects). A fourth voice
needs a DDR and probably doesn't exist.

## Scale ladder (Inter unless noted)

30 display (connect title, Fraunces on end screens) / 16-18 section
titles (700) / 14 emphasized row (700) / 13 default body / 12 dense
body & buttons in panels / 11 captions & hints / 10 whispers
(`pc-text-faint` pair). Minimum functional size is 10 at 1024x600 -
nothing interactive may label itself below 11.

## Numbers (the money rules)

- **Tabular figures wherever a number can change while visible**
  (`FontFeature.tabularFigures()`): cash, timers, VP, bids, banks.
  A timer that wobbles as digits change reads as jitter; tabular is
  calm. Non-negotiable.
- Amounts carry `$` and no decimals (the economy is integer);
  thousands run solid up to 4 digits, use thin separators only if the
  economy ever exceeds 5 digits (mod-dependent - decide per surface,
  consistently).
- Valence on numbers = sign + colour, always both (`+120` sage,
  `-450` oxblood); never colour alone.
- A TOTAL updates only after its cause has arrived (the chit lands,
  THEN the panel total changes) - typography obeys the money rule;
  totals never animate digit-by-digit (no odometer effects: that's
  casino register).

## Tables & lists

Numbers right-aligned, tabular; labels left; one hairline under
headers, none between rows (spacing separates); rows are 4-px-grid
heights. The event log is a list, not a table: sentence-cased,
timestamp-free (order is the information; times belong to match
history surfaces).

## Names

Player names: Inter 500, in the player's pawn colour ONLY on seat
markers/tags (elsewhere `pc-text`) - identity colour is spatial, not
global. Property names: Source Serif 4 on parchment, sentence case,
never truncated with "..." on the board (wrap or shrink; a cut name is
an unreadable card).

## Motion & text

Text does not animate (no typewriter, no per-letter effects). Text
ARRIVES with its container (banner slide, card flip) and then is
still. The single exception: the chit's amount travels because the
chit does.

## Accessibility

Respect platform text scaling up to 1.3x without layout breakage in
panels (the board's paper labels may cap scaling - the log/log
sentences are the accessible copy). Contrast: `pc-text` on `pc-bg`
and `parchment-ink` on `parchment` both clear WCAG AA for body sizes;
`pc-text-faint` is decorative-only by definition - never sole carrier
of needed information.
