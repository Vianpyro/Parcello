# Roadmap & product readiness

A realistic path from today's state to a commercial release, with the
product-gap inventory that drives it. This is ADVICE with reasoning,
not commitments: the owner sets priorities and version numbers (the
workspace sits at 0.19.0; bumping it on main tags and ships a release -
release.yml - so version choices ARE release decisions here).

Ground truth as of 2026-07: the v2 ruleset is DONE and playable end to
end; server, CLI, and one Flutter client (desktop+web) exist; ranked
(per-server), spectators, a bots showcase, onboarding coach marks,
i18n FR/EN, Steam-shaped packaging, and a deployment guide all exist.
The single biggest unknown is not code: **the game has never had a real
multi-human playtest cycle.**

## Phases

### Alpha - "strangers can play a full evening" (current focus)
Goal: validate fun + stability with real humans on the public server.
1. Deploy the identity provider on the owner's server
   (compose-deploy.yml is written; Rauthy client id `parcello`) and run
   the web OIDC flow for real (repays debt D2).
2. Organized playtests (the CLAUDE.md roadmap already names this the
   next priority). Instrument nothing new: the post-game survey
   (`feedback`) and SQLite history are the telemetry.
3. Fix what playtests surface in game feel - the known spec/build gaps
   are already ranked in motion-language.md section 13 (sealed-bid
   input anchored to the lifted tile is #1; AFK auto-play marker; trade
   animations).
Risks: playtests may invalidate tuning (bid window, blitz clock) -
that is their purpose; constants are one-line changes.

### Beta - "the loop retains people"
Entry condition: alpha says the core is fun.
1. Audio pass (the four category earcons are silent placeholders -
   assets/sfx/README.md).
2. Ranked UI in Flutter (queue screen, MMR on the player card - the
   protocol and visual-identity spec are ready; repays part of D5).
3. Match history surfaced to players (the data already exists in
   `--history`; needs a read path - ADR-0005 anticipates a SQLx adapter
   if queries grow).
4. Remove HS256 after the playtests (D1 - owner's stated criterion).
5. Localization beyond FR/EN if community shows up (gen-l10n makes each
   language one ARB file).

### 1.0 - "chargeable quality"
1. Real multiplayer soak at load (dozens of concurrent rooms).
2. Steam release: packaging exists (release.yml builds Steam-depot
   archives incl. Deck-fit Linux); what's missing is store presence,
   Steamworks integration decisions (overlay? achievements? - see gap
   table), and QA passes on Windows/macOS builds.
3. Documentation freeze pass (D4: fold ADR amendments into
   architecture.typ).
4. Mobile (Android/iOS) is explicitly OWNER-POSTPONED - do not let it
   creep into 1.0 by enthusiasm; the layout floor work makes it a
   bounded project later.

### Post-1.0 themes (each is ADR-first)
- **WASM mods (V2)**: the trait seam exists (`ModPlugin`); unblocked by
  MSRV 1.96; the architecture doc's V2 constraints (pure engine, dyn
  strategies, async event bus registration) were maintained for exactly
  this.
- **Global ladder / cross-server stats**: blocked on signed results
  (ADR-0009 stats note; ADR-0034 defers it). This is the largest
  architectural addition on the horizon - identity-service extension +
  result signing + aggregation service.
- **Tournaments, parties (queue with friends)**: session-layer features
  over existing seams.
- **Replay viewer**: the format is already stored; a client-side player
  over `(seed, commands)` is a self-contained project and a great
  first-contributor magnet.

## Product gap inventory

Priorities: Critical (blocks the next phase), High (retention/quality
lever), Medium (nice, bounded), Low (someday). Complexity S/M/L.
Impact = expected effect on a real player's session.

| Gap | Prio | Cx | Impact / note |
|---|---|---|---|
| Real playtest cycle + acting on it | Critical | M | Everything downstream depends on it |
| Identity deploy + web OIDC QA (D2) | Critical | S | Only web sign-in path |
| Sealed-bid input anchored to tile (motion-language #13) | High | M | The most-felt UX gap in the core loop |
| AFK auto-play marker | High | S | "The server played for you and nothing told you" |
| Audio pass (4 earcons) | High | S | Outsized feel-per-effort |
| Ranked Flutter UI | High | M | Ranked exists but is CLI-only today |
| Match history UI | High | M | Retention; data already captured |
| Trade animations | Medium | M | motion-language #13 |
| Spectate-by-code in the menu | Medium | S | Wire+CLI support exists; menu offers server-pick only |
| Cancel-ack message for ranked queue (D5) | Medium | S | Needed by ranked UI anyway |
| Reconnect UX polish (auto-rejoin last room) | Medium | S | Tokens already persisted client-side |
| Achievements / progression | Medium | M-L | Per-server only until signed results; read guide #12 before starting |
| Colour-blind palette audit | Medium | S | visual-identity.md palette was validated, but not for CVD |
| Tutorial: guided bots game | Low | L | Coach marks + spectating cover onboarding for now |
| Steamworks deep integration (overlay, cloud saves) | Low | M | Nothing to cloud-save server-side; decide scope at 1.0 |
| Anti-AFK penalties / leaver penalties | Low | M | ADR-0034 deferred; population-dependent |
| In-game reports/moderation | Low | M | Deliberately absent (no admin plane); revisit only with scale - would be an ADR reversing a documented stance |

## Dependencies that order everything

playtests -> tuning + feel fixes -> beta features;
identity deploy -> guest-off servers -> meaningful ranked population ->
ranked UI value; signed results -> ANY global feature. Do not reorder
around the first arrow: features built before playtests are bets,
features built after are answers.
