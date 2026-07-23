# Technical debt register

Every KNOWN debt, why it was accepted (debt taken knowingly is a
decision; debt discovered later is a bug in this file), the trigger
that should cause repayment, and what goes wrong if ignored. Severity:
**Critical** (repay before the trigger, no exceptions), **Important**
(schedule when the trigger fires), **Minor** (opportunistic).

Last full review: 2026-07 (post ADR-0035). When you repay one, delete
its entry in the same change.

## Critical

### D1. HS256 shared-secret auth still exists (ADR-0003)
- **Why accepted**: it predates the EdDSA path and the owner explicitly
  deferred removal until LAN/WAN playtests have exercised the real OIDC
  flow (2026-07 decision, recorded in CLAUDE.md roadmap).
- **Risk**: any party holding the secret mints identities for that
  trust domain; it also normalizes a server-holds-secrets pattern that
  ADR-0009 exists to kill. Tests mint HS256 tokens (`tests/ws.rs`) - a
  removal must swap them to a test EdDSA issuer or a test-only verifier.
- **Trigger**: first successful real-world playtest cycle on EdDSA.
- **Effort**: small code (delete `Hs256Verifier`, the `alg` dispatch
  arm, env var), medium tests (mint Ed25519 in tests instead - the
  `ed25519-dalek` dep is already there).

### D2. Web OIDC popup flow never exercised against a real IdP
- **Why accepted**: needs a deployed Rauthy + real browsers; only
  compilation is verified (CLAUDE.md rough surfaces). Safari popup
  policies are the known hazard.
- **Risk**: the ONLY sign-in path on web silently broken in production.
- **Trigger**: before announcing any public server that disables guests.
- **Effort**: one manual QA matrix (3 browsers), or a day for a
  playwright harness against a docker Rauthy.
- **Not a concern**: ADR-0037's renewal. Rauthy was confirmed (2026-07)
  to return `refresh_token` for the `parcello` client already, and the
  browser's CORS posture toward the token endpoint is exercised by web
  login itself - `oidc_login_web.dart` calls `discover` and
  `exchangeToken` cross-origin from the page during sign-in, and the
  refresh is the same endpoint, origin and method. If web login works,
  renewal's transport works. (An earlier revision of this entry claimed
  the login flow did not exercise CORS; that was wrong - the popup only
  carries the *authorize* redirect, while the token exchange is a direct
  POST from the page.)

## Important

### D3. Fuzzer floor/market-event latent coupling
- **What**: `game_state_fuzzer.rs` computes its bid floor from the LIST
  price; legal only because fuzzer content has `market_events: vec![]`.
  A future fuzzer content with a price *boom* makes generated bids
  sub-floor -> generator panics ("fuzzer generated rejected command").
- **Why accepted**: fuzzing market events was out of scope for the
  universal-floor amendment; the coupling is documented (INVARIANTS
  "enforcement gaps").
- **Trigger**: the change that adds market events to fuzzer content.
- **Effort**: small - compute the effective price in the generator.

### D4. architecture.typ is a DRAFT that ADRs have amended
- **What**: the constitution still says SQLx (superseded by ADR-0005),
  a custom developer-hosted Identity Service (amended by ADR-0009 to
  "any Ed25519 OIDC provider"), `DicePolicy` (removed by ADR-0017),
  "no central matchmaking" (deviated by ADR-0034/0035 with ADRs, as
  required).
- **Why accepted**: the repo's doctrine is explicitly "architecture.typ
  AS AMENDED BY the ADRs" (CLAUDE.md precedence). Rewriting the
  constitution wholesale risks losing rationale and breaking the
  amendment trail; the ADRs are the diff history.
- **Trigger**: a 1.0 documentation freeze, or when the amendment count
  makes first-read comprehension fail (subjective; ~40+ ADRs).
- **Effort**: medium; when done, fold ADR outcomes in AND keep the ADRs
  (they carry the WHY that a clean rewrite always loses).

### D5. Ranked matchmaking policy rough edges (ADR-0034 known-accepted)
Three, recorded at review time and left deliberately:
  - anchor head-of-line blocking (an atypical-rating oldest entry can
    stall a compatible table behind it);
  - `queued {size}` doubles as the cancel acknowledgement (ambiguous
    for richer UIs);
  - a re-queue resets the waiting credit (widened window + fallback).
- **Why accepted**: population near zero at launch; policy constants
  are deliberately constants-not-flags until playtests say otherwise.
- **Trigger**: first real complaints about queue times, or building the
  Flutter ranked UI (which will want an unambiguous cancel ack -
  additive message, cheap).
- **Effort**: each is small and independent.

### D6. Client-side settings ride the reconnect-token store
- **What**: `_issuer`, `_locale`, `_hints` live as reserved keys in the
  reconnect-token map (session.dart carries the "split into a prefs
  file" ponytail comment). Three tenants is past the comment's own
  threshold. Also: widget tests calling `resetHints()` write to the
  developer's real store file (harmless today, surprising).
- **Trigger**: the NEXT client setting, or the first test flake traced
  to store contents.
- **Effort**: small (a prefs file beside the token file + migration of
  three keys).

### D7. Spectator "last connection wins" replaces silently
- **What**: rewatching from a second connection replaces the first
  entry; the first client keeps a live socket that receives nothing
  (routing keys fixed the dangerous disconnect-shadowing; the UX gap
  remains - no "you were replaced" signal).
- **Why accepted**: mirrors the seats' rejoin doctrine; harm is
  cosmetic.
- **Trigger**: spectator-facing UI polish pass.

## Minor

### D8. Net-worth & market-price formula mirrors (invariant C3)
Triplicated by design for display (server + 2 clients), guarded only by
cross-referencing comments. A conformance test (golden values through
all three) would mechanize it. Effort: small, needs a tiny Dart<->Rust
fixture convention.

### D9. Bots ignore victory points (ADR-0020 note)
Economic heuristic only; bots under-value group completion and
conglomerates relative to the actual win condition. Accepted at
ADR-0020 ("tune at playtests"). Trigger: bots visibly losing every VP
race in showcase games that spectators watch (it is now on display).

### D10. `--turn-timeout` CLI help says "decline"
`main.rs` doc text for the canonical action still lists "decline",
which ADR-0018 removed. Text-only fix; batch with the next flag change.

### D11. No mechanized engine-purity check (INVARIANTS E1 gap)
A `cargo deny` ban-list scoped to `parcello-engine` would close it.
Effort: small; do it the next time deny.toml is open.

### D12. `docs/animation-sync.md` overlaps ADR-0028/0030 + motion-language.md
Three documents share the animation-contract story; they agree today.
Trigger: next animation-contract change - consolidate to motion-language
as the single reference (it already claims that role) and reduce the
others to pointers.

## Non-debts (things that look like debt but are decisions)

Listed so nobody "fixes" them: engine clone-per-apply; JSON wire; no
DB migration framework; no benches; no per-IP limiting in-process; no
admin control plane; CLI untested; settings-not-mod-set per room
(ADR-0015); jail cards as counts. Rationale lives in
docs/performance.md, docs/security-model.md, and the ADRs cited in
docs/domain-model.md's "deliberate simplifications".
