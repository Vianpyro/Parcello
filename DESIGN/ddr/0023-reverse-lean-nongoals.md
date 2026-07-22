# DDR-0023: chat, shop, and levels/XP enter scope as placeholders (reverses DDR-013)

Status: DECIDED (owner, 2026-07). **Reverses DDR-013.**

## Context

DDR-013 recorded "no chat / no shop / no XP-passes" as identity-tier
non-goals (reversal explicitly requires a DDR + ADR). The validated global
mockup (2026-07) features all three prominently: a CHAT control, a BOUTIQUE
/ currency, and a player LEVEL ("NIVEAU 24") with an XP bar. The game-screen
refonte (DDR-0021) therefore cannot honour the mockup without either
dropping these or reversing DDR-013. The owner has chosen to reverse it.

This is the "extraordinary reason" DDR-013 required: an explicit owner
product decision to move Parcello's identity from lean-and-mechanical
toward the mockup's richer, progression-and-social framing.

## Problem

Bring chat, shop/currency, and levels/XP into the design scope, in a way
that (a) is honest about their non-existence, and (b) does not smuggle a
backend or a monetisation commitment in through the UI.

## Alternatives

1. **Keep DDR-013, drop the three from the refonte.** Pros: preserves the
   lean identity; the mockup's own ROADMAP band already defers them.
   Rejected by the owner in favour of the mockup's richer direction.
2. **Reverse DDR-013, build them for real now.** Rejected: each is a large
   backend + protocol effort (chat: moderation/transport; shop: payments,
   an economy; levels/XP: a progression system with gameplay meaning) -
   out of a client refonte, and premature before playtests.
3. **Reverse DDR-013, ship as honest visual placeholders (decided).** In
   scope for the look now; real later, each behind its own ADR.

## Decision

DDR-013 is REVERSED. Chat, shop/currency, and levels/XP are **in design
scope as non-functional visual placeholders** in the refonte, under these
honesty rules (the SELF_CRITIQUE "never imply unsent/real state" wall):

- A placeholder is visibly inert - disabled, or badged "coming soon" - and
  never accepts input that looks like it did something.
- No fake data presented as real: a level/XP bar is decorative and must not
  claim a real standing; a shop shows no purchasable/priced state that
  could read as live.
- Each becomes a real feature only via its **own ADR** (chat transport +
  moderation; shop economy + payments; XP/level progression + its gameplay
  meaning) - and its own follow-up DDR for the real UX.

This DDR governs ONLY these three former non-goals. The other mockup
elements that were never non-goals - avatars/portraits, trade
counter-offer, replay entry, the houses/hotels two-tier VISUAL (a rendering
of the single build ladder, top level shown as a hotel) - are ordinary
placeholders needing no reversal. The rank/MMR badge stays **per-server
framed** (DDR-0012 unchanged): no global-league promise.

## Trade-offs

- Parcello's brand shifts from the lean non-goals identity toward
  progression/social. Accepted deliberately by the owner.
- Placeholders risk reading as shipped features; the honesty rules above
  are the mitigation, and they are testable (inert = not interactive).
- Monetisation (shop) is now on the visible roadmap surface; the ADR that
  makes it real is a separate, heavier product decision.

## Consequences

- DDR-013's index row is marked REVERSED by DDR-0023; its Identity-tier
  listing is removed.
- DESIGN_PHILOSOPHY's non-goals section is updated (chat/shop/XP no longer
  categorical non-goals; now deferred-real placeholders).
- The refonte may render these regions (player bar level/XP, nav-rail chat,
  a shop entry) as inert placeholders per DDR-0021.
- Three future ADRs are implied (chat, shop, progression) - none created
  now; the UI must not pretend they exist.

## Review date

When any of the three is proposed as a real feature (its ADR), or at the
next identity review.
