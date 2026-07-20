# DDR-0019: the design system's public API is a stability contract

Status: ACCEPTED (owner, 2026-07)

## Context

DDR-0016 put the design system in-tree (`lib/design/`, realized
incrementally - today `tokens.dart`, `typography.dart`, `motion.dart`;
tomorrow `components/`, `composite/`, `theme.dart`). Its exported surface
is consumed by the ENTIRE app (`lib/ui/` and the top-level widgets).
DDR-0016 also kept a clean extraction path to a package, gated on
criteria. For the boundary to be worth anything - and for the eventual
package version to mean something - the public surface has to be treated
as an API, not as incidental helpers.

## Problem

How stable is the design system's public API, and what may change it?

## Decision

**Anything the design system EXPORTS is public API, designed to last.**
Today that is:

- `tokens.dart`: `Pc` (colours, spacing scale, radius, shadows), the
  `groupColors` / `pawnColors` maps, `pawnColor()`;
- `typography.dart`: `PcText`;
- `motion.dart`: `Motion`, `Tier`, `Lane`, `MotionProfile`;
- and, as they land, every `PcButton`/`PcPanel`/... component and its
  props.

The rule:

- **Internal implementation may change freely** - how a token is
  computed, how a component is built, private helpers, file layout.
- **The public INTERFACE** - the member names, their signatures, and the
  SEMANTICS the rest of the app relies on - **changes only with a
  documented necessity**: a DDR, or an equivalent written justification
  in the change itself.
- **Additions are free** (and encouraged) - a new token, role, or
  component is new public API, so DESIGN it to last (name it well, give
  it the states/variants the bible calls for), but it needs no DDR to
  exist.
- **Renames, removals, signature changes, and semantic changes** to an
  existing public member DO need the DDR / justification - because they
  churn call sites and break the trust that lets people build on the
  system.

## Trade-offs

- A little upfront care naming and shaping public members, and the
  discipline of a note when you must break one. In exchange: `lib/ui/`
  call sites do not churn under the design system, the system is
  trustworthy to build screens on, and the eventual package extraction
  (DDR-0016) inherits a surface whose version actually means something.
- This is the design twin of the technical repo's additive-only wire
  discipline (INVARIANTS P2): evolve by adding, break only on purpose,
  in the open.

## Consequences

- The design-system source files carry a short stability note in their
  header pointing here.
- DESIGN_REVIEW.md and AI_ENGINEERING.md add the check: a diff that
  renames/removes/changes a `lib/design` public member (or shifts its
  semantics) must carry a DDR or an in-diff justification; a pure
  addition does not.
- Not machine-enforced: a public-API SNAPSHOT test would also flag
  additions (which are allowed), producing noise for a solo/small team.
  This stays a review-only rule, like most of the bible's contracts -
  the named guards (C2) cover the mechanical parts.

## Review date

Standing rule. Revisit if the design system is extracted to a package
(DDR-0016), at which point a real semver policy replaces "a DDR".
