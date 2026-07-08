# Parcello visual identity

Status: direction note for the Flutter client (`clients/flutter`), the
packaged game. Companion to `docs/business-tour-direction.md` (rules)
and ADRs 0017-0024 (the v2 ruleset). The embedded web client stays a
debug surface; it may borrow the palette but is not held to this spec.
No engine impact anywhere in this document.

Sources: the two validated mockups (main menu, isometric board), the
"board game doctrine" section of the tokens.css shared as inspiration,
Business Tour screenshots as a LAYOUT reference only, and the decisions
of 2026-07 recorded inline below.

## Art direction

Clean, squared, quietly opulent - an Art Deco "city of progress"
register (think Arcane's Piltover) WITHOUT copying any protected
design: the inspiration applies through geometry and restraint, not
through hextech iconography or Arcane assets. Decided 2026-07: the
validated palette below stays authoritative; Piltover shows up in the
SHAPES.

- Sharp corners: border radius 0-2 px everywhere (buttons, cards,
  tiles, dialogs). No pills, no soft blobs.
- Symmetry and framing: thin double-line frames, stepped/tiered motifs,
  fine gold hairlines on dark surfaces. Ornament is structural, never
  clutter - when in doubt, remove it.
- Flat colour only: no gradients, no grain or noise textures, no
  shadows heavier than a 1-2 px hairline. Closer to a vintage travel
  poster than to photorealism (also a renderer-friendly constraint).
- Signature element: property tiles as small card-stock index cards -
  parchment face, thin ink rule, the group colour as an edge band on
  one side, never a full fill. Special corners and chance tiles break
  the pattern (solid gold / dark surface) so the board stays scannable
  at a glance.

## Palette

| Token | Hex | Usage |
| --- | --- | --- |
| `pc-bg` | `#14171C` | App background. Replaces the current `0xFF1C1F26` (minor shift, one line in main.dart). |
| `pc-surface` | `#1E2229` | Cards, menu tiles, chance tiles on the board. |
| `pc-surface-2` | `#262B33` | Raised elements (avatar, hover, dialogs). |
| `pc-border` | `#33383F` | Hairlines on dark surfaces. |
| `pc-border-muted` | `#3A4048` | Dashed borders ("coming soon" zones). |
| `pc-text` | `#ECE6D8` | Primary text on dark - warm off-white, never pure white. |
| `pc-text-muted` | `#8C8577` | Secondary text. |
| `pc-text-faint` | `#655F52` | Quiet labels ("unranked", captions). |
| `pc-parchment` | `#ECE0C2` | Property tile faces, receipts, "paper" elements. |
| `pc-parchment-ink` | `#2A2420` | Text on parchment - warm black, never pure black. |
| `pc-gold` | `#D8B45A` | Primary accent: CTAs, currency amounts, special board tiles. Same value as the existing theme seed. |
| `pc-gold-dark` | `#A9812F` | Pressed state / strong gold borders. |
| `pc-oxblood` | `#9C433A` | Danger / aggressive action (takeover, bankruptcy alert). |
| `pc-sage` | `#3F5240` | Positive (group completed, gain); also the board's centre plaza. |

### Property group colours

The 8 colour groups of `mods/base/data/properties.toml` (verified), in
muted tones rather than classic Monopoly brights:

| Group | Hex |
| --- | --- |
| brown | `#6B4A3A` |
| lightblue | `#52708A` |
| pink | `#8F5566` |
| orange | `#AB6A3D` |
| red | `#7D3D33` |
| yellow | `#A68B3C` |
| green | `#3F6B52` |
| navy | `#2E3A5C` |

The base mod also has a ninth group, `resort` (the two `group_scaled`
tiles). Resorts are not one of the eight: render them as parchment
cards with a full `pc-gold` edge band - proposal, to confirm on the
first board render.

### Pawn colours (up to 6 players)

Deliberately distinct from the group colours so "whose pawn" never
reads as "which group": `#4A7D7A` (teal), `#B5654A` (coral - both
already in the board mockup), `#7A6A9C` (violet), `#7F8A4A` (olive),
`#5C728F` (slate), `#9C5C74` (rose).

## Typography

Three roles, deliberately not one family:

- **Display / wordmark** - Fraunces 700. The word "Parcello" and
  end-screen titles only; never functional text.
- **Body / UI** - Inter 400/500/700. Buttons, amounts, timers, labels.
  Tabular figures (`FontFeature.tabularFigures()`) wherever a number
  ticks live: cash, timers, victory points.
- **Tile labels** - Source Serif 4 for property names on the board
  (quieter register than Inter, without Fraunces's weight at small
  sizes).

All three are OFL; BUNDLE them as Flutter assets (pubspec `fonts:`
section, licence texts committed alongside). The desktop/Steam client
must work offline - no Google Fonts fetching at runtime. (Corrects the
web-oriented note in the source draft.)

## Localization

Decided 2026-07: French AND English from the start via Flutter
gen-l10n (l10n.yaml + two ARB files), no hardcoded UI strings; the
scaffolding lands with the first restyled screen. Repo, code and docs
stay English. Property names come from mod data (plain ASCII, a data
concern); corner tiles are localized by KIND in the client (Go / Free
Parking / Jail / Go To Jail and their French equivalents) - Parcello's
own vocabulary, never the reference games' trade-dress names.

## Screen: loading

Deliberately minimal: `pc-bg`, the wordmark in Fraunces with its gold
hairline, a simple fade if any animation at all. Built not to be
watched - fast loads make it a flash.

## Screen: main menu

Business-Tour-derived layout (player card on top, two main tiles,
secondary links), fully re-themed, minus everything Parcello has no
concept for. Decided 2026-07:

- **Player card**: avatar + name. NO MMR yet - a rating needs the
  matchmaking service, which does not exist; the card gains the number
  when the service lands (algorithm and tiers deferred with it).
- **Private table** - Create / Join by 5-letter code, label "unranked".
  The only fully functional path today; the existing connect flow
  (server URL, LAN discovery, server manager) keeps its place here -
  this pass restyles, it does not remove flows.
- **Play (ranked)** - GREYED, dashed `pc-border-muted`, "coming soon",
  alongside Tutorial, Board editor and Tournaments. Solo = solo queue,
  not vs bots; a practice-vs-AI mode (engine `bot.rs`) deserves its own
  slot later, likely near the tutorial.
- **Secondary links**: Discord, release notes.
- **Deliberately dropped** from the reference layout: tickets/energy,
  Gold Pass, fortune wheels - no corresponding mechanics, and energy
  gating contradicts "short games, always accessible".

## Screen: board

Isometric diamond perspective (per the mockup). Content spec:

- Property tiles: parchment index cards, group-colour edge band, name
  in Source Serif 4.
- Chance tiles: `pc-surface` dark, a lone "?" mark.
- Corners: solid `pc-gold`, localized by tile kind (see Localization).
- Resorts: parchment with the full gold edge band (palette note).
- Centre plaza: `pc-sage`, minimal fine-line motif, no texture.
- Blitz clock above the board: Inter tabular figures on a dark chip
  with a gold hairline; shows the 12 s ring, then the time-bank drain
  (ADR-0023).
- Pawns: flat simple shapes in the pawn palette.
- Mockup tile names ("vieux-port", "cascades", ...) are illustrative
  placeholders, not content proposals.

The v2 ruleset needs HUD space on this screen from day one - mock these
into the board scene BEFORE building the isometric renderer so nothing
fights for room later:

- victory-point counters per player (ADR-0020, the race is the game);
- the two shared pool counters (ADR-0019, the doom-clock fuse);
- the 3-slot market forecast strip (ADR-0021);
- every player's movement hand, public (ADR-0017);
- the 5 s sealed-bid overlay (ADR-0018) and the jail choice dialog
  (ADR-0024), both under the 12 s rhythm.

## Flutter implementation notes

Nothing in this pass ships code; scale markers only.

```dart
theme: ThemeData(
  brightness: Brightness.dark,
  scaffoldBackgroundColor: const Color(0xFF14171C), // pc-bg
  colorScheme: ColorScheme.fromSeed(
    seedColor: const Color(0xFFD8B45A), // pc-gold, unchanged seed
    brightness: Brightness.dark,
  ).copyWith(
    surface: const Color(0xFF1E2229), // pc-surface
    error: const Color(0xFF9C433A),   // pc-oxblood
  ),
  fontFamily: 'Inter',
  // Art direction: sharp corners everywhere - override the shape of
  // cards, buttons and dialogs with RoundedRectangleBorder(
  //   borderRadius: BorderRadius.circular(2)).
),
```

- `MenuScreen` (main.dart) is a re-theme plus the content changes
  above - no structural rewrite.
- The isometric board is a separate engineering effort: `board.dart`
  renders flat today; a projected board (CustomPainter or a transform
  stack) plus hit-testing is its own chantier, estimated apart from the
  palette work.
- gen-l10n scaffolding lands with the first restyled screen.

## Open questions

- MMR/ranking algorithm and tiers - deferred with the matchmaking
  service.
- Where the practice-vs-AI mode lives in the menu.
- Icon set: Tabler icons (MIT, bundling fine) plus flat in-house
  glyphs, INSTEAD of Business-Tour-style mascot illustration - a real
  register change to confirm on the first styled screen, not a detail.
