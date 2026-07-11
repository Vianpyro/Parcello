# ADR-0025: Flutter Web replaces the hand-written browser client

Status: accepted

## Context
The project maintained two independently-authored browser-facing UIs:
`clients/flutter` (the Dart client, desktop-only until now) and
`crates/server/web/index.html` (a single hand-written HTML/JS file,
embedded into the server binary via `include_str!`). Keeping them in
feature parity was manual and already drifting - `docs/architecture.typ`
itself specifies Flutter as the one client, targeting "Android, iOS,
macOS, Windows, Linux, and web" from a single source tree, so the
hand-written file was always the undocumented deviation, not the
intended design. It also never grew real OIDC support (only a pasted
identity token), unlike the Flutter client's PKCE login.

Flutter Web compiles to a directory of static assets (`index.html`, JS
bundles, a multi-MB CanvasKit/WASM renderer, fonts, `assets/`) rather
than one file, so it can't be embedded the same way. Two ways to serve
it were considered: compile the whole directory into the server binary
(e.g. `rust-embed`), or resolve it from disk at runtime. This codebase
already answers that exact question for a different generated,
directory-shaped artifact: `mods/` is resolved at boot from
`--mods-dir`/`PARCELLO_MODS_DIR`, ships as a sibling directory in every
release tarball and the Docker image, and fails loudly at startup
(`parcello_mods::resolve(...)?`) if it's missing. Compiling the web
build in instead would couple the Rust build (and the `ci.yml`
`stable`/`msrv` jobs, whose only job is Rust-toolchain validation) to a
Flutter/Dart toolchain, and force `release.yml`'s `server` job to
rebuild an architecture-independent artifact redundantly on every
OS/arch leg of its matrix (including arm64).

## Decision
- `clients/flutter` gains the `web` platform
  (`flutter create --platforms=web .`) and becomes the one client for
  desktop (Windows, Linux, macOS) and browser. `crates/server/web/`
  (the hand-written client) is deleted.
- The server serves the Flutter Web build from disk at runtime, mirroring
  `--mods-dir`: a new `--web-dir`/`PARCELLO_WEB_DIR` arg (default `web`),
  `tower-http::ServeDir` mounted as the router's fallback service (the
  explicit `/healthz` and `/ws` routes still take precedence), and a
  boot-time check that fails loudly - same idiom as mod resolution - if
  `<web-dir>/index.html` is missing, rather than booting into a broken
  or empty root route.
- Four Dart files are platform-specific and split via Dart's
  compile-time conditional export (`dart.library.js_interop`), since
  `dart:io` does not exist in the web compilation target at all -
  `kIsWeb` alone cannot fix a file that fails to compile:
  - `oidc.dart`: the native flow (`oidc_login_io.dart`) opens the system
    browser and listens on a loopback port (impossible in a browser
    sandbox); the web flow (`oidc_login_web.dart`) opens a synchronous,
    pre-navigation popup and completes via `postMessage` from a small
    static redirect target, `web/oidc-callback.html`. Both share PKCE/
    JWT/discovery helpers (`oidc_common.dart`, now over `package:http`
    instead of raw `dart:io` `HttpClient` so `discover()` stays
    portable) behind one public API. The identity provider needs a
    second registered `redirect_uri` for the web origin, alongside the
    native `http://127.0.0.1:*` one (same client id, `docs/deployment.md`).
  - `lan_discovery.dart` / `server_manager.dart`: UDP multicast discovery
    and local process spawning have no browser equivalent at all (no raw
    sockets, no process spawn in a sandbox) - the web build gets a
    never-reached stub, and `main.dart` hides both menu entries behind
    `kIsWeb`.
  - `session_storage.dart`: the reconnect-token/issuer persistence
    (ADR-0008) moves from direct `File` I/O in `session.dart` to a file
    on desktop / `window.localStorage` on web, chosen over adding a
    `shared_preferences` dependency to stay consistent with the
    project's minimal-runtime-dependency style.
- `release.yml` gains a platform-independent `web` job (built once,
  `flutter build web --release`) whose artifact is downloaded into every
  `server` matrix leg and packaged as `web/` inside each release tarball,
  same as `mods/` already is. The Docker image builds its own Flutter
  Web client in a self-contained stage instead (a manual, checksummed
  SDK install from Google's official release CDN, not a third-party
  image, to keep every stage traceable to an official source like the
  Rust/Debian ones) - `docker build .` stays independent of CI-produced
  artifacts, matching its existing "never blocks the binary release"
  design. `ci.yml`'s `stable`/`msrv` jobs are untouched: they never
  touch Flutter.

## Consequences
- Every deployment channel (bare-metal tarballs, the Docker image,
  local `cargo run`) now needs a populated `web/` next to the server
  binary, the same operational shape `mods/` already has - documented in
  `docs/deployment.md` and the README Quickstart.
- `release.yml`'s `server` job now depends on the new `web` job
  (`needs: [version, web]`), a new coupling: a broken Flutter Web build
  blocks the Rust server/CLI release too. Accepted over the alternative
  (shipping a tarball with an empty `web/`, which the fail-loud boot
  check would then refuse to start).
- The web OIDC popup/`postMessage` flow is not realistically
  CI-testable (headless-browser popup scripting is unreliable, no real
  user gesture) and has not been exercised against a real identity
  provider - required manual QA before relying on it in production,
  tracked in CLAUDE.md's "Untested / rough surfaces". The native flow
  and its test (`test/oidc_test.dart`) are unchanged.
- "Browse public games" (LAN discovery) and "Server Manager" are
  desktop-only features now; the web build's menu never shows them.
