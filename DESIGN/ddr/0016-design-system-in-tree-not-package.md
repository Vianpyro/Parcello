# DDR-0016: the design system lives in-tree (`lib/design/`) for now - package extraction DEFERRED, not refused

Status: ACCEPTED (owner, 2026-07) - a DEFERRAL of package extraction with
explicit re-evaluation criteria, NOT a permanent decision to stay in-tree.

## Context

The Design Bible (DESIGN/) describes a SYSTEM - tokens, typography,
spacing, motion, components - not a set of screens. The owner rightly
wants the code to reflect that: build the system as a first-class,
reusable layer BEFORE screens, so no component gets rewritten screen by
screen. The open question is the physical form of that layer. Bounded by:
the technical bible's overengineering guidance ("no seam a second
consumer doesn't need"), invariant C2 ("a hex/duration literal at a use
site is a bug"), and the fact that a large part of the system already
exists in `lib/` (`tokens.dart`, `motion.dart`, `stage/director/overlay`).

## Problem

Should the design system be a separate Dart/Flutter package
(`packages/parcello_design/` + `packages/parcello_theme/`, consumed by
the app via a path/workspace dependency), or an in-tree layered folder
(`lib/design/`)?

## Alternatives considered

1. **Separate package(s)** (the owner's initial proposal). Pros: a hard
   API boundary, independent versioning, obvious reuse if a second app
   appears. Cons: there is ONE consumer today; a second pubspec +
   workspace wiring + `flutter.yml` CI changes + rewriting imports across
   ~45 green-tested files, for zero user-facing gain; it front-loads the
   cost of a seam nothing yet uses.
2. **In-tree layered folder** `lib/design/` (recommended). Pros: same
   foundations-first sequencing and reuse; the boundary is enforced by
   convention (C2) + a lint; no restructure, no CI churn, no import
   rewrite; a clean extraction path remains. Cons: the boundary is
   convention, not compiler-enforced across a package edge (a determined
   use-site could still reach past it).
3. **Do nothing / keep ad-hoc** - rejected: it is exactly the
   screens-not-system drift this roadmap exists to prevent.

## Decision

Adopt **in-tree `lib/design/`** with the sub-structure in
IMPLEMENTATION_ROADMAP Phase 0 (`tokens` / `typography` / `motion` /
`theme` / `components` / `composite` / `animation`), screens in `lib/ui/`
consuming it. Enforce the boundary with C2 + a lint rule.

**This is a DEFERRAL of package extraction, not a refusal of it** (owner,
2026-07). Building in-tree now, with a deliberately clean folder boundary,
keeps the eventual extraction MECHANICAL. Extraction is expected, not
hypothetical; the only open question is WHEN, answered by the criteria
below.

## Extraction criteria (any ONE re-opens this via a follow-up DDR)

Extract `lib/design/` into `packages/parcello_design/` when any of these
becomes true. Each names the concrete signal to watch so the trigger is
checkable, not a vibe:

1. **A second consumer is scheduled.** A standalone **replay viewer**
   (the accepted-command log already replays bit-identically, ADR-0001;
   LEGACY + the product roadmap name it as the plausible first one), a
   **companion / spectator app**, a **board-editor** tool, a marketing
   microsite reusing components, or any second Flutter target. Signal: a
   second `main()` that needs the tokens/components. This is the classic
   "the seam finally earns its keep" - reactive.
2. **Design-system stabilization.** The component API (roadmap Phases
   5-6) has shipped and gone ~2 releases / ~3 months WITHOUT churning its
   public surface (no renamed/removed component params, no restructured
   token names). Signal: the `lib/design/components/` and `composite/`
   public APIs stop appearing in diffs. A stable contract is cheap and
   safe to freeze behind a package edge; a moving one is not - so
   extracting AT stabilization is the proactive path (freeze the boundary
   before a second consumer needs it in a hurry), whereas extracting at
   criterion 1 is the reactive path.
3. **The in-tree boundary is not holding.** If C2 + the lint prove
   insufficient in practice - use sites keep reaching past `lib/design/`
   into internals, or screens keep re-inventing tokens - a
   compiler-enforced package edge becomes worth its cost. Signal:
   recurring DESIGN_REVIEW findings of boundary violations that the lint
   cannot express.

Whichever fires first, the response is a short follow-up DDR that flips
this one to "superseded", performs the (mechanical) move, and wires the
package into CI. The phase ORDER in the roadmap is unaffected either way.

## Trade-offs

- We accept a convention-enforced boundary now for zero restructure cost,
  in exchange for a (cheap, mechanical) extraction WHEN one of the
  criteria fires. Because extraction is treated as expected rather than
  hypothetical, the folder boundary is kept clean from Phase 1 on -
  paying the small ongoing discipline cost so the eventual move stays
  mechanical.

## Consequences

- IMPLEMENTATION_ROADMAP Phase 0 targets `lib/design/`; every later phase
  builds there. The phase ORDER is identical either way - only Phase 0's
  bootstrap differs.
- If the owner overrides in favour of a package (e.g. replay viewer is
  imminent): add a `packages/parcello_design` bootstrap as Phase 0, wire
  the path dependency + CI, and retarget the phases at the package.
  Nothing downstream reorders. This DDR flips to "superseded by the
  package decision" and records why.
- A lint forbidding raw hex/duration/spacing literals at use sites should
  land in Phase 1 to make C2 mechanical.

## Review date

Whenever any Extraction criterion fires (watch them continuously - they
are checkable signals, not a calendar), or at 12 months as a backstop to
re-assess whether stabilization (criterion 2) has quietly been reached.
