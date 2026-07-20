# Concept critique - the worked example

In 2026-07 the project received a rich AI-generated concept image (an
isometric gold-on-navy dashboard mockup) alongside screenshots of the
real build (connect, menu, lobby/board, rules). This file records the
critique - partly for its conclusions, mostly as the METHOD every future
inspiration pass should copy: inspiration is digested through the
architecture and the philosophy, never swallowed.

## The method (apply to any future concept/mockup/moodboard)

1. List what it makes you feel before what it shows. (The feeling is
   what you are trying to keep.)
2. Check every depicted MECHANIC against the domain model and ADRs. A
   mockup that draws a rule the game doesn't have is proposing a rule
   change - route it to game design, not to the style guide.
3. Check every depicted INFORMATION against the view-privacy invariants
   (INVARIANTS E4/E5). If the mockup shows data the server masks, it is
   unimplementable, full stop.
4. Check every depicted SYSTEM against the non-goals (shops, XP,
   passes...). Non-goals are refusals, not omissions.
5. Only then extract style: shapes, density, hierarchy, materials.

## What the concept image gets RIGHT (preserve)

- **Commitment.** One register everywhere - gold hairlines, deep
  blue-charcoal, framed panels. It proves the palette can carry a whole
  product. Its confidence is the thing most worth keeping.
- **The board as centerpiece** with dimensional stepped buildings: this
  IS the isometric end state visual-identity.md points at, and the
  building silhouettes (tiered, flat-shaded, Deco) are a good concrete
  reference for that future chantier.
- **The movement hand given hero placement** (bottom-left, large,
  numbered cards): correct instinct - ADR-0017 makes the hand the one
  action of every turn, and the current build under-stages it.
- **Dense-but-framed information**: rent ladders on the property card,
  a timestamped history feed, per-seat cash always visible. The framing
  discipline (every zone ruled and titled) is what keeps density calm -
  worth copying in side-panel structure.
- **A "your move" banner with remaining-actions text**: a good, quiet
  P2 treatment.

## What it gets WRONG (and why - each is a lesson, not a nitpick)

1. **It draws an OPEN ascending auction** ("current bid 560,000", rival
   bids listed, +50K/+100K raise buttons). Parcello's auctions are
   SEALED (ADR-0018): pending bids are masked in the view (invariant
   E5); there IS no "current bid". This is the single most instructive
   error: the mockup is beautiful and *architecturally impossible*. Any
   raise-button UI must be read as bid-COMPOSITION against your own
   sealed amount - which the real build already does (quick +% chips).
2. **It invents forbidden systems**: XP level bars, a boutique, currency
   top-up framing, seasons-as-battle-pass in the roadmap footer. All
   are recorded non-goals (dropped 2026-07, DESIGN_PHILOSOPHY). A
   concept that needs a shop to fill its corners is telling you the
   layout has spare corners, nothing more.
3. **Global rank framing** ("Diamant I, MMR 2456") - ladders are
   per-server (ADR-0034); tiers are an open question deferred with the
   ranked UI. Show the number and its per-server nature; do not promise
   a global league.
4. **"TOUR 18/60"** - rounds are the hand-refill metronome (min
   `hands_cycled`), not a fixed count; the game ends by points, pool,
   clock, or survival. A turn fraction lies about the game's shape.
5. **Chat panel** - no chat exists; adding one is a moderation-surface
   ADR (the no-admin-plane stance), not a UI decision.
6. **One-screen-everything density** would not survive the 1024x600
   floor with real localized strings; the mockup's micro-text is
   already illegible at its own resolution. The real build's answer
   (scrolling side panel, board center) is the correct architecture.
7. **Register drift at the edges**: violet-glow avatars and glossy
   bevels lean toward the casino/neon register the art direction
   forbids. The palette survives; the lighting must stay flat.

## Verdict applied

Adopted into this bible: the framing discipline, the hand-as-hero
principle (SCREEN_ARCHITECTURE board rules), the isometric building
silhouettes as the reference for the future board chantier, the rent-
ladder property card content (DESIGN_SYSTEM). Rejected with recorded
reasons: everything in the wrong list. The real build's screenshots,
for their part, confirm the foundation reads correctly at real sizes -
their gaps are catalogued in COMMERCIAL_UX_AUDIT.md, not here.
