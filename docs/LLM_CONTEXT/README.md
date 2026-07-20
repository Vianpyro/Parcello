# LLM_CONTEXT

Self-contained subsystem summaries for readers with a partial context
window - human or AI. Each fits in a few pages and lets you reason about
a subsystem WITHOUT loading its sources.

**These are DERIVED documents** (invariant X3). Each header names its
sources of truth; the sources always win. If you change a source, update
its summary in the same change - a stale summary is worse than none,
because a partial-context reader cannot tell.

Written 2026-07, at workspace version 0.19.0, ADRs 0001-0035, 200 Rust
tests + 50 Flutter tests. If that snapshot is far behind you, verify
against sources before trusting details; the SHAPES here (layering,
seams, contracts) rot much slower than the numbers.

| File | Covers |
|---|---|
| architecture-summary.md | whole-system shape, layering, seams |
| engine-summary.md | pure rules crate |
| protocol-summary.md | wire format & evolution rules |
| server-summary.md | rooms, timers, auth, persistence, ranked, spectate |
| client-summary.md | Flutter client (+ CLI harness) |
| gameplay-summary.md | the v2 ruleset in play terms |
| ranked-and-spectate-summary.md | ADR-0034/0035 features end to end |
| testing-summary.md | test map & contracts |
| security-summary.md | trust boundaries & mitigations |
| release-and-ops-summary.md | CI, releases, deployment |

Deep dives: CLAUDE.md (index), docs/INVARIANTS.md (rules of the road),
docs/domain-model.md, docs/extension-guides.md, docs/AI_ENGINEERING.md.
