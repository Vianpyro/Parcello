# DDR-0018: typography roles carry size+weight+family and a DEFAULT colour

Status: ACCEPTED (2026-07) - the taxonomy decision that shapes Phase 2.

## Context

Phase 2 of the design roadmap replaces ~73 inline `TextStyle(...)` across
the client with named roles (TYPOGRAPHY.md: display/section/body/caption/
amount/...). Before migrating, one taxonomy question shapes every one of
those sites: do roles carry a colour, or only typographic properties?

## Problem

Should a `PcText` role be purely typographic (size + weight + family), with
colour applied separately, or should it carry a default colour?

## Alternatives considered

1. **Purely typographic** (colour orthogonal, always applied by the caller).
   Pros: matches COLOR_SYSTEM's "colour is a separate semantic"; one role
   serves text/muted/gold/oxblood. Cons: nearly every site becomes
   `PcText.caption.copyWith(color: ...)` - verbose, and NON-const (Flutter
   prefers const `TextStyle`), a real ergonomic + perf cost at 73 sites.
2. **Role carries its DOMINANT default colour, overridable** (chosen). Pros:
   the common case is a clean `const` (`PcText.caption` == 11px muted, the
   dominant caption); an atypical colour is an explicit
   `.copyWith(color: ...)` that READS as intentional (an oxblood body should
   stand out in the diff); const-preserving. Cons: a role is no longer
   purely typographic - but its default IS the semantic default, so this
   reinforces COLOR_SYSTEM rather than fighting it.

## Decision

Roles carry **size + weight + family + a sensible default colour** (their
dominant one). Callers override colour via `.copyWith(color:)` only for the
exceptions. Some roles deliberately OMIT `fontSize` (e.g. `wordmark`,
`amount`) so they inherit the ambient size where that is the point.
`amount` additionally carries `FontFeature.tabularFigures()` (TYPOGRAPHY.md:
live numbers never jitter).

## Trade-offs

- A role bundles a default colour; atypical colours are explicit overrides.
  Accepted: it keeps `const`, reads clean, and makes the unusual visible.
- Migration is NOT purely value-preserving the way spacing was: it surfaces
  real inconsistencies (e.g. the connect-screen wordmark renders in Inter
  while the menu wordmark is Fraunces; several one-off sizes 15/18/20 have
  no obvious role). These are flagged, not silently "fixed" - aligning them
  is a visual decision (a follow-up DDR / owner intent), never a side effect
  of a mechanical pass.

## Consequences

- `lib/typography.dart` defines `PcText` with the role set; it grows as
  migration proceeds (roles are added when a real recurring combo appears,
  not speculatively).
- Migration is incremental and value-preserving PER SITE: a site migrates
  only when a role matches its exact (size, weight, family, colour) - or
  differs only by colour (an explicit override). Sites that would change
  pixels (the Inter wordmark, inherited-size bodies) stay bespoke and are
  logged as follow-ups.
- The C2 guard extends to typography (flag inline `TextStyle` with a raw
  `fontSize`) only once coverage is high - same warning-then-error
  progression as spacing.

## Review date

When the typography migration reaches high coverage (fold in the flagged
inconsistencies then), or if the default-colour ergonomics prove wrong in
practice.
