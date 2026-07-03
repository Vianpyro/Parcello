# Parcello

Open-source multiplayer board game in the spirit of Business Tour / Monopoly.
Authoritative Rust server, thin clients, community-hosted servers
(Minecraft model), data-driven mods.

This repository is the playable backend: pure game engine, TOML mod layer,
WebSocket server with an embedded browser client, and a terminal test
client. A richer Flutter client and the Global Identity Service are
separate future components.

## Workspace

| Crate                | Layer (architecture doc)  | Contents                                             |
| -------------------- | ------------------------- | ---------------------------------------------------- |
| `parcello-engine`    | Game Engine (section 4)   | Pure, synchronous rules. No I/O, no async, no rand.  |
| `parcello-mods`      | Mod Layer (section 7)     | TOML bundles, Registry merge, `ModPlugin` trait.     |
| `parcello-protocol`  | Transport contract        | JSON message envelopes shared by server and clients. |
| `parcello-server`    | Transport + Session (5)   | Axum WS server, rooms, auth, history, web client.    |
| `parcello-cli`       | Test harness              | Terminal client to exercise the server end-to-end.   |

Patterns from the doc and where they live:

- Command: `engine::command`, single `Engine::apply` pipeline (ADR-0001).
- Observer: `engine::event::Event`, broadcast by rooms after each command.
- State Machine: room `Lobby -> Active -> Finished` (`server::room`),
  turn `AwaitRoll -> AwaitBuy -> AwaitEnd` (`engine::state::TurnPhase`).
- Registry: `mods::RegistryBuilder` freezes into validated `GameContent`.
- Strategy: `DicePolicy`, `RentCalculator`, `BankruptcyResolver` behind
  `dyn` in `Engine` (V2 WASM substitutes implementations here).
- Repository: `server::history::GameHistory` port with two adapters:
  in-memory (default) and SQLite via a writer thread (`--history`, ADR-0005).
- Plugin: `mods::ModPlugin` (`on_load`/`on_unload`); V1 ships the TOML
  implementation, V2 adds a Wasmtime-backed one behind the same trait.

## Quickstart

Rust 1.75+.

```sh
cargo build --workspace
cargo test  --workspace

# Server (guest auth, LAN/testing only)
cargo run -p parcello-server -- --insecure-guest
```

**Play in a browser:** open `http://localhost:7878/`, enter a name, leave
the code empty to create a room (or paste a code to join), share the code.
The client is a single embedded HTML file speaking the same protocol as the
CLI; the server stays the only authority. It has not been exercised in a
real browser inside the development sandbox (protocol coverage is verified
mechanically against the engine's command and event enums), so expect
cosmetic rough edges.

**Or with the terminal client:**

```sh
# Terminal 2: host
cargo run -p parcello-cli -- --name alice --create
# prints: room created: ABCDE

# Terminal 3: guest
cargo run -p parcello-cli -- --name bob --join ABCDE

# In the host terminal:
#   start
# then per the prompts: roll | buy | no | bid <n> | pass | build <t>
#                       | sell <t> | mortgage <t> | redeem <t> | pay | end | resign
# trading (any time):   offer <seat> <give$> <tiles|-> <want$> <tiles|->
#                       accept <id> | refuse <id> | cancel <id>
```

Server flags: `--bind 0.0.0.0:7878`, `--mods-dir mods`, `--mod base`
(repeatable, ordered; later mods override earlier ones per key),
`--insecure-guest`, `--history <file.db>` (SQLite game logs; omit for
in-memory, see ADR-0005), `--turn-timeout <secs>` (auto-play the pending
canonical action - roll/decline/pass/end turn - for a player who stalls
that long; 0 = disabled, the default). Set `PARCELLO_JWT_SECRET` to accept
HS256 tokens with `{sub, name, exp}` claims (ADR-0003).

Docker: `docker build -t parcello . && docker run -p 7878:7878 parcello`
(mount a volume and add `--history data/parcello.db` for persistence), or
pull the published image: `ghcr.io/vianpyro/parcello-server`.

## Releases

Bumping the workspace `version` in `Cargo.toml` on `main` triggers
`.github/workflows/release.yml`: it tags `vX.Y.Z`, builds the server + CLI
for Linux and Windows (with the `mods/` directory bundled), builds the
Flutter Windows client, attaches everything to an auto-generated GitHub
release, and pushes the server image to GHCR (`vX.Y.Z` + `latest`). Keep
`clients/flutter/pubspec.yaml`'s version in step - it stamps the client
executable. Re-pushing without a bump is a no-op.

## Protocol (v0, JSON over WebSocket at `/ws`)

Client -> server: `create {auth, mods?}` (optional ordered mod list for
the room, ADR-0006; omit for the server default), `join {code, auth}`,
`start`, `cmd {cmd}`, `ping`. Server -> client: `room_created`, `joined` (includes
the resolved mod bundle and, mid-game, a state snapshot), `lobby`,
`game_started`, `update {events, view}`, `rejected {error}` (sent only to
the offending player), `error`, `pong`. Shapes live in `parcello-protocol`;
commands and events are the engine's own serialized types, so the wire
format is the replay format.

## Mods (V1, data-only)

A mod is a directory under `--mods-dir`:

```
mods/<id>/
  manifest.toml          # id, version, author, min_game_version
  data/properties.toml   # [[tile]] board definitions
  data/cards.toml        # [[chance]] / [[community]]
  data/rules.toml        # [rules] named scalar overrides
```

Base game content is itself a mod (`mods/base`), loaded first. Merge is
last-loaded-wins per key: tiles and cards replace in place by id, rule
scalars override by name; every conflict is logged at WARN. Unknown rule
keys are ignored with a warning. The resolved bundle is pushed to clients
on join, so clients never need mod files locally.

Each room can pick its own ordered mod list at creation (ADR-0006): the
clients expose a "mods" field on create (CLI: repeatable `--mod`), and an
omitted or empty list selects the server's boot-time default set. Mod ids
are allowlist-validated server-side. `mods/highroller` is a rules-only
example (richer, faster games): create a room with `base, highroller`.

V1 hook points: `rules.{starting_balance, go_salary, jail_fine,
max_houses_per_property, bankruptcy_threshold, auction_on_decline}`
(booleans as 0/1), `cards.chance[*]`,
`cards.community[*]`, `properties[*]` (including per-tile `rent_model`:
`houses` (default), `group_scaled` for stations, `dice_scaled` for
utilities; the scaled models need no `house_cost` and cannot be built on).

## Game rules implemented

Movement with Go salary, property purchase offers, trading (asynchronous
offers of cash and/or house-free-group tiles between any solvent players,
re-validated at acceptance so stale offers reject without side effects;
blocked during auctions to preserve the auction's solvency invariant;
capped at 4 open offers per proposer; offers are public in the client
view), auctions on declined
purchases (round-robin left of the decliner, strict raises capped by cash,
high bidder skipped until outbid, no bids leaves the tile unsold; disable
with `rules.auction_on_decline = 0`), rent (full-group doubles
unimproved rent; a singleton group counts as full; stations scale by
stations owned, utilities by dice total), building and voluntary house
sales with the classic even-build/even-sell rule (forced liquidation
follows it too), the full-group requirement, per-tile cap, and
no-mortgage-in-group rule, mortgage (half
price out, plus 10% interest floored to redeem; mortgaged tiles collect
nothing but still count for group ownership; a group must be house-free to
mortgage), taxes, chance/community decks (cyclic, seeded shuffle, chained
card moves capped at depth 4), jail (doubles escape, fine, forced fine on
the third failed roll), get-out-of-jail-free cards (held as a per-player
count, spent voluntarily before rolling or automatically instead of the
forced third-roll fine; the decks are immutable cyclic shuffles, so drawn
cards never leave the rotation), doubles grant an extra roll and three consecutive
doubles jail you, partial-payment bankruptcy with liquidation (houses at
half cost first, then automatic mortgages, highest value first) and asset
transfer to the creditor (mortgages carry over as-is; the bank refurbishes
returned tiles), resignation, last-player-standing win.

Deliberate V1 simplifications: no immediate interest charge when mortgaged
tiles change hands (trades and bankruptcy transfer them as-is);
get-out-of-jail cards are a count, not tradeable objects, and stay in the
deck rotation once drawn.

## Known MVP limitations

- Rooms with no connected seat dissolve after 30 minutes idle; there is no
  persistence, so a dissolved game is gone.
- Guest identities are spoofable by design (`--insecure-guest`).
- History is in-memory unless `--history` is set; the SQLite adapter logs
  `(seed, ordered accepted commands)`, i.e. complete deterministic replays.
- No reconnect resume token: rejoin is by identity (same guest name/JWT sub).
- The AFK timer (`--turn-timeout`) is off by default; without it a stalled
  player blocks the game until the room idles out.

## Deviations from the architecture doc

See `docs/adr/`: 0001 `apply` returns `Result`; 0002 PRNG seed inside
`GameState`; 0003 interim auth (guest + HS256 behind `IdentityVerifier`);
0004 server-wide mod set (room `Starting` state collapses to a point);
0005 rusqlite writer thread instead of SQLx behind `GameHistory`;
0006 per-room mod sets at creation (amends 0004, `Starting` stays
collapsed).

## Roadmap

Flutter client polish (a Windows-desktop client lives in `clients/flutter`,
see its README); Global Identity Service (asymmetric JWT, JWKS); WASM
(Wasmtime) mod plugins behind `ModPlugin`; private trade offers;
reconnect tokens; richer history queries (stats) if needed.
