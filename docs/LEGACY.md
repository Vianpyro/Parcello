# To the next maintainer

Written 2026-07, at 0.19.0, by the AI collaborator that built the v2
ruleset era's later features (ranked, spectate, the UX pass) alongside
the owner, as its final contribution. You - human or model - now hold a
codebase whose most valuable property is not any feature but a working
SYSTEM for changing it safely. This letter explains the spirit; the
mechanics live in the documents it cites.

## Philosophy (the WHY behind everything)

**Goal**: a fast, dynamic, legible multiplayer board game -
Business-Tour's energy, not Monopoly's attrition - that a community can
run itself forever. Sessions of 10-15 minutes where every landing is
contested and every economic lead is visible on the board
(docs/business-tour-direction.md is the design bible).

**Non-goals** (protect these refusals): MMO-style scaling of one
server; developer-hosted gameplay infrastructure; pay-to-anything or
energy gates (explicitly dropped from the menu spec); trusting
community hosts; feature flags multiplying untested configurations.

**Audience**: friends-and-communities multiplayer first (the Minecraft
hosting model), commercial distribution planned (hence permissive-only
licenses, Steam-shaped packaging, the polish bar).

**Architectural philosophy**: purity where determinism pays (the
engine), actors where concurrency lurks (rooms), named boundaries where
trust ends (validators), ADRs where opinions harden into decisions. The
wire is the replay. Every deviation is written down BEFORE it is coded.

**Gameplay philosophy**: legibility beats simulation. The two 2026-07
auction amendments are the whole lesson in miniature: an invisible
discount became a visible rebate ("rewards the table cannot see do not
motivate"), and a mechanically-sound 1$-snipe died because it FELT like
a glitch ("mechanically sound but illegible" loses to "slightly less
clever but obviously fair").

**UI philosophy**: the server is truth, the stage is presentation, and
the two never share a notifier; information arrives when it is needed
(coach marks) at the place it concerns (rejections shown ON the tile
that refused); motion is budgeted, skippable, and never gates input
(docs/motion-language.md).

**Multiplayer philosophy**: the server is authoritative, the table is
never hostage to one client (every wait is capped), and absence is
handled by canonical actions that never spend a player's money.

**Extensibility philosophy**: variability enters through named seams
only; content is data (mods); a new seam is an ADR. V2 (WASM) was
designed-for years early by keeping the engine pure and strategies
behind `dyn` - preserve that option even if you never exercise it.

## What must be preserved (in order of pain-if-lost)

1. The replay identity: pure engine, non-mutating rejections, wire ==
   replay (docs/INVARIANTS.md E1-E3, P1). Everything else can be
   rebuilt from a surviving replay contract; nothing can be rebuilt
   without it.
2. The masking discipline: privacy is a property of VIEWS, never of
   clients (E4/E5).
3. The actor room and session-owned time (S1, S4-S5).
4. The untrusted-host stance (security-model.md boundary 2).
5. The governance loop itself: architecture.typ + ADR amendments +
   CLAUDE.md index + INVARIANTS. The documents are load-bearing; a
   maintainer who stops writing ADRs inherits an unmaintainable repo
   within a year.

## What SHOULD evolve (do not embalm these)

Tuning constants (windows, banks, matchmaking policy) once real
playtests speak; the bot heuristic (it ignores VP - debt D9); the
matchmaking rough edges (D5); HS256's deletion (D1); the client's
missing feel items (motion-language section 13); architecture.typ's
eventual amendment-folding (D4). The ruleset itself may evolve -
through ADRs, with the fuzzer and same-seed guard extended in the same
breath.

## Mistakes to avoid (each nearly happened or happened)

- "Simplifying" the two timed windows into one shared primitive
  (ADR-0024 explains why they are deliberately parallel twins).
- Counting non-activity as activity in rooms (the Probe bug: one
  innocent-looking line would have made every room immortal).
- Letting an identity string do double duty (spectator disconnects
  shadowing seats - fixed with routing keys; the general lesson:
  session routing ids and domain identities are different types even
  when they look like the same string).
- Trusting author-context review alone: BOTH deep bugs above were
  found only by fresh-context verification against the ADR. Make that
  practice survive (AI_ENGINEERING review step 6).
- Fixing a "bug" that is a documented decision (the 1$-snipe was
  documented; it still needed CHANGING - but via an amendment, not a
  patch; the process is what kept the fuzzer/bot/tests coherent).

## Known regrets

- The three settings tenants squatting in the reconnect-token store
  (D6) - the ponytail comment said "split when more settings appear"
  and we added a third tenant instead of splitting.
- No mechanized engine-purity check (D11) - a review-only invariant is
  a debt to every future reviewer.
- The web OIDC flow shipped compilation-tested only (D2). It is the
  single most user-visible untested path.
- Formula mirrors guarded by comments instead of a conformance test
  (C3/D8).
- ADRs 0001-0016 predate the "alternatives considered" habit that
  0034 models; their alternatives are lost. (Do NOT retro-fill them
  with invented history - just do better going forward.)

## Lessons learned (the transferable ones)

1. Documentation debt compounds faster than code debt here, because
   the docs are the change-control system.
2. Every invariant needs a NAMED guard (test or type); "everyone
   knows" lasts one maintainer generation.
3. Additive-only wire evolution is cheap the day you adopt it and
   priceless every day after.
4. The compiler is the best reviewer: exhaustive matches (relay,
   describeEvent) and struct literals in tests turn omissions into
   build failures. Prefer that over discipline.
5. Playtests outrank theory: three separate 2026-07 amendments (bid
   window 5s->12s, the rebate, the universal floor) came from watching
   humans, not from analysis.

## Practical advice

Read CLAUDE.md, then INVARIANTS, then the recipe for your task. Run
the gates before claiming anything. Verify with fresh context. Write
the ADR first when in doubt. Update the derived docs in the same
change. When you learn something the hard way, spend the extra ten
minutes putting it in AI_ENGINEERING's pitfalls or the debt register -
that habit is this project's actual technology.

---

# Self-critique (the departing author reviewing their own legacy)

Honesty section. What follows is what a sceptical principal engineer
should poke at first.

**Where the documentation is still insufficient**
- The Flutter client's internals (director lanes/coalescing, stage
  beat grammar) are summarized but not INVARIANT-ized; motion-language
  .md carries them narratively. A C-series expansion of INVARIANTS for
  the director contract would help the next animation change.
- `docs/animation-sync.md` vs ADR-0028/0030 vs motion-language.md
  triple-tell one story (D12); I documented the overlap instead of
  resolving it - resolving it risked losing nuance I could not verify
  against the original author's intent (the owner's).
- The mods TOML schema has no reference doc beyond examples in
  `mods/base` and architecture.typ's sketch; guide #9 points at
  examples rather than a schema. Fine until third-party modders exist.
- No sequence diagrams. Prose state machines (domain-model.md) carry
  the content, but a reconnect-mid-auction diagram would beat three
  paragraphs.

**Assumptions I made**
- That per-server ladders and the untrusted-host stance remain wanted
  as the project grows (all security/product advice leans on it).
- That the owner's priority ordering in CLAUDE.md's roadmap still
  holds (roadmap-and-product.md extrapolates from it).
- That current constants (windows, caps, budgets) survive playtests
  well enough that documenting exact values helps more than it will
  mislead. Numbers in LLM_CONTEXT will rot first - the READMEs warn,
  but warnings are weaker than absence.

**Remaining uncertainties**
- Whether the fuzzer's legality-mirroring design scales as rules grow
  (generator/engine agreement is O(rules) duplicated judgment; a
  "generate-and-filter-rejections" fuzzer mode might age better -
  that is an ADR-worthy debate I am NOT settling here).
- Whether the coach-mark set is the right five (playtests will say).
- Real-world behavior of the web OIDC flow (D2) - unverified, stated
  as such everywhere, still the weakest load-bearing claim.

**Questions future maintainers should revisit deliberately**
1. When the amendment trail on architecture.typ folds (D4), what is
   the constitution's new single narrative? Plan a week, not an hour.
2. Should `finish_on_time` remain the only out-of-log step forever, or
   should a general "server verdict" record enter the log format?
   (Touches P1; decide before a second out-of-log step tempts anyone.)
3. Is `RoomSettings` carrying full `RuleParams` sustainable as rules
   multiply, or does the settings wire need versioned profiles?
4. At what community size does "no admin control plane" stop being a
   feature? (The stance is documented; its expiry condition is not.)

**Given one more week, in order**
1. Mechanize E1 (engine-purity deny ban-list) and add the
   paused-clock idle/Probe regression test - converting the two
   riskiest review-only invariants into guards.
2. A golden-value conformance test for the C3 formula mirrors,
   Rust + Dart from one fixture file.
3. Playwright harness for the web OIDC popup against a dockerized
   Rauthy (retiring D2 and the "manual QA per release" tax).
4. The prefs-file split (D6) with a three-key migration.
5. A `mods/` schema reference doc generated from the serde types.

Everything on this list is deliberately SMALL - because the honest
final assessment is that this codebase's architecture is sound and its
biggest remaining risk is neither code nor design: it is that the
documentation system that keeps it sound stops being fed. Feed it.
