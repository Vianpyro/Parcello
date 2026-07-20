# Architecture summary

Sources of truth: docs/architecture.typ (as amended by docs/adr/*),
CLAUDE.md.

## What Parcello is

An open-source multiplayer board game (Business-Tour-style: fast,
dynamic, 10-15 minute sessions - NOT Monopoly's slow accumulation).
Authoritative Rust server; thin clients that only render and relay
input; community-hosted servers (the Minecraft model - the developer
hosts nothing but, optionally, an identity provider); data-driven TOML
mods now, WASM mods later.

## The five layers (dependencies point down only)

```
Transport   axum HTTP/WS upgrade, parse, size/rate caps      crates/server/src/ws.rs
Session     auth-once, rooms, timers, ranked, spectate       crates/server/src/{room,auth,ranked,showcase}...
Engine      pure rules: apply(state, cmd) -> (state, events) crates/engine
Mods        TOML registry merge -> validated GameContent      crates/mods
Persistence Repository ports: GameHistory, RatingStore       crates/server/src/{history,ranked/store}.rs
```

One deliberate exception: engine <-> mods is bidirectional by design
(events down, V2 hooks may queue commands up).

## The load-bearing ideas (memorize these five)

1. **The engine is pure and rejections don't mutate** -> the wire IS
   the replay: `(players, seed, accepted commands)` replays
   bit-identically. Everything - history, debugging, future
   anti-cheat, WASM mods - leans on this.
2. **Views are projections masked server-side** (`for_seat` /
   `for_spectator`): sealed bids, votes, and trades are private in the
   VIEW, because clients are untrusted renderers.
3. **One Tokio task owns each room**; all access is message-passing
   (`RoomCmd`). No locks around game state, ever.
4. **Time lives in the session layer only.** The engine has no clock;
   server timers inject ordinary commands (auto-bids, AFK moves) so
   replay integrity survives. Animation pacing meets server timers at
   the ack watermark (seq/`animation_done`, capped at 10s) - the game
   clock alone is never gated.
5. **Community servers are untrusted** - nothing sensitive rides in
   tokens, ladders are per-server, and no feature may require trusting
   a host (global stats need signed results first).

## Variability seams (extend HERE, don't invent)

Strategy traits in the engine (`RentCalculator`, `BankruptcyResolver`),
`ModPlugin` (V2 = Wasmtime impl behind it), `IdentityVerifier` (guest /
EdDSA-JWKS / deprecated HS256), Repository ports (`GameHistory`
append-only; `RatingStore` read-modify-write), `RuleParams` scalars
(host-clamped per room), the additive JSON wire, the `RoomCmd` actor.

## Governance

architecture.typ is the constitution; deviations REQUIRE an ADR
(docs/adr/0001..0035 so far); CLAUDE.md indexes both and lists hard
constraints; docs/INVARIANTS.md catalogues what must never change.
Precedence: ADR (newer) > architecture.typ text it amends.
