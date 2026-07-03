// Pin to a specific version; check https://typst.app/universe/package/fletcher for latest
#import "@preview/fletcher:0.5.9": diagram, node, edge

// ── Document metadata ────────────────────────────────────────────────────────

#set document(
  title: "Parcello - System Architecture",
  author: "Parcello Contributors",
)

// ── Page layout ──────────────────────────────────────────────────────────────

#set page(
  paper: "a4",
  margin: (x: 2.5cm, y: 3cm),
  numbering: "1",
  header: [
    #set text(size: 9pt, fill: luma(150))
    Parcello - System Architecture
    #h(1fr)
    v0.1 DRAFT
    #v(-6pt)
    #line(length: 100%, stroke: 0.4pt + luma(200))
  ],
)

#set text(font: "New Computer Modern", size: 11pt)
#set par(justify: true, leading: 0.65em)
#set heading(numbering: "1.1.")

#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  v(0.4em)
  it
}

// ── Title page ───────────────────────────────────────────────────────────────

#align(center + horizon)[
  #text(size: 32pt, weight: "bold")[Parcello]
  #v(0.4cm)
  #text(size: 20pt, fill: luma(80))[System Architecture]
  #v(2cm)
  #table(
    columns: (auto, auto),
    stroke: none,
    align: (right, left),
    gutter: (0pt, 4pt),
    [Document series:], [Architecture],
    [Section:], [1 of 6],
    [Status:], [DRAFT],
    [Version:], [0.1],
  )
]

#pagebreak()

// ── Table of contents ────────────────────────────────────────────────────────

#outline(depth: 2, indent: 1.5em)

// ── 1. Overview ──────────────────────────────────────────────────────────────

= Overview

Parcello is an open-source multiplayer board game modelled after Business Tour.
Its defining architectural requirement is *first-class moddability*: the system
must support community content replacement at every stage of its life, from
TOML-only data overrides in V1 to full WASM scripting in V2.

All authoritative game logic resides on the server. Clients are stateless
renderers: they display a projection of server-side state and relay player
input as serialized commands. The client never validates game rules.

== Design Goals

#table(
  columns: (30%, 1fr),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Goal*], [*Rationale*],
  [Authoritative server],
  [Single source of truth; prevents cheating; enables deterministic replay and
   clean reconnection without client-side resimulation.],
  [Thin client],
  [Flutter codebase targets desktop and mobile from a single source tree with
   no game logic duplication.],
  [Data-driven mods (V1)],
  [Low barrier for community content; no compilation required for TOML mods.],
  [Scriptable mods (V2)],
  [Full behavior override via WASM; Garry's Mod-level extensibility.],
  [Self-hostable game servers],
  [The game server is a distributable binary. Community members host their own
   instances; the developer hosts nothing except the Global Identity Service.],
  [Minimal developer infrastructure],
  [Only the Global Identity Service is developer-hosted. Its load is
   proportional to logins and join events, not to active gameplay.],
  [Open-source, auditable],
  [Permissively licensed dependencies only; no proprietary SDKs.],
)

== Scope

This document defines the system boundary, component responsibilities, layer
architecture, and the design patterns that enforce maintainability and
moddability. WASM scripting internals (V2) are referenced where they constrain
current design decisions; full V2 specification is deferred to its own document.

// ── 2. System Components ─────────────────────────────────────────────────────

= System Components

#table(
  columns: (20%, 22%, 1fr),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Component*], [*Technology*], [*Responsibility*],
  [Client],
  [Flutter (Dart)],
  [Render projected game state; capture and dispatch player input; load mod
   assets received from server; display event animations.],
  [Game Server],
  [Rust (Axum + Tokio)],
  [Distributable binary; community-hosted. Authoritative game engine; room and
   session lifecycle; command validation and application; event dispatch; mod
   registry. Verifies player identity against the Global Identity Service.],
  [Global Identity Service],
  [Rust (Axum + SQLite)],
  [Developer-hosted lightweight service. Bridges OAuth providers to stable
   global player IDs. Issues signed player JWTs accepted by all game servers.
   Stores display name, provider binding, and public profile.],
  [Game Engine],
  [Rust (internal crate)],
  [Pure, deterministic state machine. Given a `GameState` and a `Command`,
   returns `(GameState, Vec<Event>)`. No I/O, no async, no side effects.],
  [Mod Registry],
  [Rust (internal crate)],
  [Loads and merges mod bundles at room creation. Populates typed registries
   (properties, cards, rules). Exposes the event bus hook interface.],
  [Persistence],
  [SQLite via SQLx],
  [Player accounts, session tokens, game history. Abstracted behind
   `Repository` traits to allow a future Postgres migration.],
  [Auth],
  [OAuth2 (Discord + Google) \ via Global Identity Service],
  [Players authenticate once with OAuth; the Global Identity Service issues a
   global player JWT. Game servers verify this JWT via a single lightweight
   call to the Identity Service. No password storage.],
  [Mod distribution (MVP)],
  [Host -> clients at join],
  [Server operator loads mod bundles from disk; the server serializes and
   pushes the resolved bundle to joining clients. No central registry.],
)

// ── 3. High-Level Architecture ───────────────────────────────────────────────

= High-Level Architecture

#figure(
  diagram(
    node-stroke: 0.7pt,
    node-corner-radius: 3pt,
    spacing: (22mm, 18mm),
    // Clients
    node((0, 0), [Flutter Client \ #text(size: 9pt)[(Desktop)]], width: 34mm),
    node((2, 0), [Flutter Client \ #text(size: 9pt)[(Mobile)]], width: 34mm),
    // Game server (community-hosted)
    node((1, 1), [
      *Game Server* \
      #text(size: 9pt, style: "italic")[(community-hosted)]
    ], width: 38mm, fill: luma(232)),
    // Server internals
    node((0, 2.5), [Game Engine], width: 29mm, fill: luma(242)),
    node((1, 2.5), [Mod Registry], width: 29mm, fill: luma(242)),
    node((2, 2.5), [Auth + Sessions], width: 29mm, fill: luma(242)),
    // Local persistence
    node((0, 4), [SQLite \ #text(size: 9pt)[(local to server)]], width: 29mm),
    // Global identity (developer-hosted)
    node((2, 4), [
      *Global Identity Service* \
      #text(size: 9pt, style: "italic")[(developer-hosted)]
    ], width: 38mm, fill: luma(220)),
    // OAuth
    node((2, 5.5), [OAuth Providers \ #text(size: 9pt)[(Discord + Google)]], width: 36mm),
    // Edges
    edge((0, 0), (1, 1), "<->", label: [WS]),
    edge((2, 0), (1, 1), "<->", label: [WS]),
    edge((1, 1), (0, 2.5), "->"),
    edge((1, 1), (1, 2.5), "<->"),
    edge((1, 1), (2, 2.5), "->"),
    edge((0, 2.5), (0, 4), "->"),
    edge((1, 2.5), (0, 4), "->"),
    edge((2, 2.5), (2, 4), "<->", label: [verify / register]),
    edge((2, 4), (2, 5.5), "->", label: [OAuth bridge]),
  ),
  caption: [
    System overview. Game servers are community-hosted binaries; only the
    Global Identity Service is developer-operated. Double-headed arrows
    indicate bidirectional communication.
  ],
)

The only developer-operated infrastructure is the Global Identity Service.
Its traffic is proportional to login events and player joins, not to active
gameplay — making it viable on a free tier or a minimal VPS.

Game servers are distributed as a standalone binary attached to each GitHub
release. The client never holds canonical game state; on reconnection the
server pushes the full current projection to the rejoining client. The Mod
Registry is populated once at room creation and is immutable for that room's
lifetime.

// ── 4. Backend Architecture ──────────────────────────────────────────────────

= Backend Architecture (Rust)

The server is organized into five strictly layered modules. Dependencies flow
*downward only*. The single architectural exception is the event bus link
between the Game Engine and Mod Layer, which is bidirectional by design
(engine emits events down; mod hooks may queue commands back up).

#figure(
  diagram(
    node-stroke: 0.7pt,
    node-corner-radius: 3pt,
    spacing: (0pt, 16mm),
    node((0, 0), [
      *Transport Layer* \
      #text(size: 9pt)[Axum routes - WebSocket upgrade - TLS (reverse proxy)]
    ], width: 115mm, fill: rgb("#dbeafe")),
    node((0, 1), [
      *Session Layer* \
      #text(size: 9pt)[OAuth verification - JWT issuance - Room registry - Connection management]
    ], width: 115mm, fill: rgb("#ddd6fe")),
    node((0, 2), [
      *Game Engine* \
      #text(size: 9pt)[State machine - Command processor - Event bus - Rule execution]
    ], width: 115mm, fill: rgb("#d1fae5")),
    node((0, 3), [
      *Mod Layer* \
      #text(size: 9pt)[Plugin loader - TOML registry - Hook interface - (V2: Wasmtime host)]
    ], width: 115mm, fill: rgb("#fef3c7")),
    node((0, 4), [
      *Persistence Layer* \
      #text(size: 9pt)[Repository trait - SQLx executor - Schema migrations]
    ], width: 115mm, fill: rgb("#fee2e2")),
    edge((0, 0), (0, 1), "->"),
    edge((0, 1), (0, 2), "->"),
    edge((0, 2), (0, 3), "<->", label: [events / hooks]),
    edge((0, 3), (0, 4), "->"),
  ),
  caption: [
    Backend layer diagram. Dependencies flow downward only. The bidirectional
    edge represents the event bus: engine emits events, mod hooks may enqueue
    commands in response.
  ],
)

=== Transport Layer

Routes HTTP and WebSocket connections. Responsibilities are strictly limited
to protocol-level validation, WebSocket upgrade, and request deserialization.
No game logic. Delegates immediately to the Session Layer after parsing.

=== Session Layer

Verifies incoming player JWTs issued by the Global Identity Service via a
lightweight HTTP call on first connection. Issues short-lived local session
tokens for subsequent WebSocket frames. Maintains a concurrent
`HashMap<RoomId, RoomHandle>`; each handle owns a dedicated Tokio task for
one room.

=== Game Engine

*Hard invariant: pure and synchronous.*

```rust
fn apply(state: GameState, cmd: Command) -> (GameState, Vec<Event>)
```

No I/O, no async, no side effects. This purity enables:
- Deterministic replay from a command log.
- Unit testing without an async runtime.
- Future WASM-side re-execution of rule overrides (V2 constraint).

=== Mod Layer

Loads mod bundles at room creation. Merges TOML data into typed registries.
Manages hook subscription tables for the event bus. In V2, this layer also
hosts Wasmtime instances and maps WASM exports to event subscriptions.

=== Persistence Layer

All database access is gated behind `Repository` traits defined in a `ports`
module. The concrete SQLx implementation lives in an `adapters` module. No SQL
appears outside this layer. Migrations are managed with `sqlx-cli`.

// ── 5. Frontend Architecture ─────────────────────────────────────────────────

= Frontend Architecture (Flutter)

The client holds a *projected* copy of `GameState` derived entirely from
server-sent `StateUpdate` messages. Command submission may be optimistic for
animation purposes, but the server result is always authoritative.

#table(
  columns: (22%, 1fr),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Layer*], [*Responsibility*],
  [UI Layer],
  [Widgets, animations, theming. Reads from state providers; emits
   user-intent events. No business logic.],
  [State Layer (Riverpod)],
  [Holds projected `GameState`, connection status, and local UI state.
   Updated exclusively by the Network Layer. Drives reactive widget rebuilds.],
  [Network Layer],
  [WebSocket client with automatic reconnection and exponential backoff.
   Serializes outgoing `Command` structs; deserializes incoming
   `ServerMessage` envelopes.],
  [Mod Loader],
  [Receives and caches the mod bundle pushed by the server at join time.
   Populates a local `AssetRegistry` (textures, sounds). Pure display data;
   no game logic.],
)

// ── 6. Design Patterns ───────────────────────────────────────────────────────

= Design Patterns

The following patterns are *architectural mandates*, not guidelines. Any
deviation requires an Architecture Decision Record (ADR).

#table(
  columns: (15%, 17%, 1fr, 20%),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Pattern*], [*Location*], [*Role in Parcello*], [*Moddability impact*],
  [Command],
  [Game Engine],
  [Every player action is a serializable `Command` variant. All commands pass
   through a single `CommandProcessor` pipeline: validate -> apply ->
   emit events. No action bypasses this pipeline.],
  [Pre/post hooks per command type (V2). Full audit log derived trivially
   from the command stream.],
  [Observer / Event Bus],
  [Engine <-> Mod Layer],
  [After each state transition the engine emits typed `Event` values.
   Subscribers react without coupling to engine internals. Subscriptions are
   registered at room creation time.],
  [Primary mod hook surface. V1 mods receive events passively; V2 mods may
   enqueue commands in response.],
  [State Machine],
  [Room + Turn],
  [Room: `Lobby -> Starting -> Active -> Finished`. Turn: `Roll -> Action ->
   End`. All transitions are explicit and exhaustive; invalid transitions
   are rejected at the type level where possible.],
  [New states or extended transition guards injectable by mods (V2).
   Explicit states prevent silent invalid transitions.],
  [Registry],
  [Mod Layer],
  [`PropertyRegistry`, `CardRegistry`, `RuleRegistry` are populated at room
   creation from merged mod data, keyed by string identifier. The engine
   resolves all content through registries; no hardcoded game data.],
  [Mods register content by providing TOML conforming to the registry schema.
   Duplicate keys follow last-loaded-wins; conflicts logged at `WARN`.],
  [Strategy],
  [Rule Engine],
  [`RentCalculator`, `BankruptcyResolver`, `DicePolicy` are traits with
   provided default implementations. Concrete types are injected at room
   creation via the Mod Layer; the engine calls through the trait.],
  [Mods substitute alternative implementations via WASM export binding (V2).
   Default implementations remain independently testable.],
  [Repository],
  [Persistence Layer],
  [All data access is behind a trait. Concrete implementations are injected at
   startup. No SQL in business logic layers.],
  [Enables test doubles; allows SQLite -> Postgres migration with zero
   business logic changes.],
  [Plugin],
  [Mod Loader],
  [Each mod bundle implements a `ModPlugin` interface: `id()`, `version()`,
   `on_load(registries)`, `on_unload()`. Called at room creation and
   teardown respectively.],
  [Stable integration point for all V1 and V2 mods. The loader is the only
   component aware of the mod lifecycle.],
)

// ── 7. Moddability Architecture ──────────────────────────────────────────────

= Moddability Architecture

== V1 - Data-Driven (TOML)

A V1 mod is a directory with the following layout:

```
<mod-id>/
  manifest.toml          # id, version, author, min_game_version
  data/
    properties.toml      # property tile definitions
    cards.toml           # chance and community chest card definitions
    rules.toml           # named rule parameter overrides
  assets/                # optional; omit if mod is data-only
    board.png
    piece_<n>.png
```

At room creation the server: (1) resolves the active mod list for the room,
(2) calls `ModPlugin::on_load` for each mod in dependency order, (3) merges
registries (last-loaded-wins per key, conflicts logged at `WARN`), then
(4) serializes the resolved asset bundle and pushes it to each joining client.

=== Registry merge rules

- *Scalar rule parameters*: last-loaded mod wins; conflict logged.
- *Collections* (properties, cards): additive merge by key; duplicate keys
  follow last-loaded-wins.
- *Base game content* is always loaded first; mod content layers on top.

=== V1 hook points

V1 mods cannot execute code. They declare named parameter overrides that the
engine resolves through `RuleRegistry` at runtime:

#table(
  columns: (45%, 1fr),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Key*], [*Effect*],
  [`rules.starting_balance`], [Each player's starting money.],
  [`rules.jail_fine`], [Cost to exit jail without rolling.],
  [`rules.max_houses_per_property`], [Build limit before hotel threshold.],
  [`rules.bankruptcy_threshold`], [Net worth floor that triggers bankruptcy.],
  [`cards.chance[*]`], [Replace or append chance card definitions.],
  [`cards.community[*]`], [Replace or append community chest definitions.],
  [`properties[*]`], [Replace or append property tile definitions.],
)

== V2 - WASM Scripting (Deferred)

V2 allows a mod to ship a `.wasm` component hosted by Wasmtime on the server.
The constraints V2 imposes on the *current* architecture are:

+ The event bus must support async callback registration without restructuring.
+ `GameEngine::apply` must remain pure (no async, no I/O) to allow WASM-side
  re-execution of rule overrides within the sandbox.
+ All `Strategy` trait implementations must be behind `dyn` pointers to allow
  runtime substitution by WASM-exported functions.

Full V2 ABI specification is deferred to a dedicated architecture document.

// ── 8. Technology Decisions ──────────────────────────────────────────────────

= Technology Decisions

#table(
  columns: (18%, 14%, 1fr, 1fr),
  stroke: 0.5pt,
  fill: (_, row) => if row == 0 { luma(230) } else { none },
  [*Decision*], [*Choice*], [*Rationale*], [*Trade-offs*],
  [Server language],
  [Rust],
  [Memory safety without GC; async performance via Tokio; type system catches
   protocol errors at compile time.],
  [Steeper contributor onboarding; smaller ecosystem than Go or Node.],
  [HTTP + WS framework],
  [Axum],
  [Ergonomic Tower middleware model; first-class WebSocket support;
   `tower-http` covers tracing, CORS, compression out of the box.],
  [Younger than Actix; fewer community middleware layers.],
  [Database],
  [SQLite via SQLx],
  [Zero infrastructure for MVP; compile-time query verification; Postgres
   migration path via Repository impl swap with no business logic changes.],
  [No horizontal scale; WAL mode required for concurrent reads.
   Acceptable for MVP load.],
  [Client framework],
  [Flutter],
  [Single codebase for Android, iOS, macOS, Windows, Linux, and web. Mature
   widget system; no game engine overhead for a board game UI.],
  [Dart is unfamiliar; larger binary than native; Flutter web canvas
   performance is variable.],
  [Client state management],
  [Riverpod],
  [Compile-safe providers; no BuildContext coupling; natural fit for reactive
   server-pushed state.],
  [Opinionated; learning curve for Dart newcomers.],
  [Mod scripting (V2)],
  [WASM via Wasmtime],
  [Language-agnostic; sandboxed by construction; Wasmtime embeds in Rust with
   a stable C API and active maintenance.],
  [ABI design is non-trivial; WASM debugging is harder than Lua; deferred.],
  [Mod distribution (MVP)],
  [Host -> clients at join],
  [Zero infrastructure; no registry to operate or fund.],
  [Bundle size bounded by WebSocket practicality; no discoverability layer;
   host is implicitly trusted by joining clients.],
  [Hosting model],
  [Minecraft-style self-hosted],
  [Game server distributed as a binary; community hosts instances. Developer
   hosts only the Global Identity Service. Hosting costs scale with the
   community, not with the developer.],
  [No central matchmaking; players must find or host a server. Mitigated by
   a community server list (future work).],
  [Authentication],
  [OAuth2 via Global Identity Service],
  [Players authenticate once globally; all community game servers accept the
   same signed player JWT. No per-server account management. Discord is the
   natural community platform for an indie multiplayer game.],
  [Dependency on the Global Identity Service being available at join time;
   offline play requires guest sessions (future work).],
)
