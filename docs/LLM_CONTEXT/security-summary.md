# Security summary

Source of truth: docs/security-model.md (full threat model with
locations); docs/INVARIANTS.md S2-S6, E4-E5.

## The two-sided trust model

Clients are untrusted (server validates everything) AND community
servers are untrusted (players choose whom to join). The second half
drives the unusual decisions: no PII/secrets in tokens (EdDSA JWTs
verified against public JWKS - servers never call the IdP), per-server
ranked ladders, and a standing rule that NO feature may require
trusting a host (global stats need signed results - deferred).

## Standing mitigations (verify they still exist before relying)

64 KiB inbound READ cap + 32/16 token-bucket per connection + 1024
global connection semaphore (ws.rs, lib.rs); per-IP limits delegated
to the reverse proxy BY DESIGN. Mod ids allowlisted (they become
filesystem paths). Room settings clamped (`limits` module). All
broadcast text stripped of control/bidi/zero-width chars; `@` handles
rejected (no email leakage). Guest mid-game seats protected by
constant-time reconnect tokens (first-join name squatting is a
DOCUMENTED residual of `--insecure-guest`). Sealed bids/votes and
trades masked in per-seat views server-side. RNG/deck never
serialized. Spectators capped at 32, auth-gated, command-refused,
timer-inert. Probes don't count as room activity (anti-immortal-room).
Parameterized SQL only.

## Accepted residuals (with reasoning in the full doc)

HS256 stopgap until real playtests (debt D1 - do not build on it);
guest name squatting; room-churn ceiling bounded by the connection
cap; CLI `--token` visible in ps (test harness only); showcase's idle
bot CPU (opt-in).

## Review triggers

New wire data reaching paths/SQL/terminal/broadcast -> named validator
+ hostile-input test. New view field -> derivable from hidden state?
New timer -> gated vs absolute, decided explicitly. Anything
persistent keyed on identity -> `player_id`, never the handle; refuse
spoofable identities. "Trust the client/server briefly" -> no.
