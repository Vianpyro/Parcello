# Release & ops summary

Sources of truth: .github/workflows/{ci,release,flutter,codeql}.yml,
.github/{release.yml,dependabot.yml}, Dockerfile, compose-*.yml,
docs/deployment.md, CLAUDE.md CI/release sections.

## CI (`ci.yml`)

Five parallel jobs aggregated by a single required `CI OK` check:
lint (fmt + typos + clippy pedantic/nursery `-D warnings`),
test+coverage (one instrumented llvm-cov run; line floor 88% after
excluding cli/, server main.rs, lan.rs; lcov+HTML artifacts),
msrv (1.96 `cargo check --all-targets --locked`),
rustdoc (`-D warnings`), deps (machete + deny: RustSec advisories,
permissive-only licenses - commercial distribution is planned,
crates.io-only sources). Docs-only paths skip CI; PR pushes cancel
stale runs; weekly cron = bit-rot/advisory canary. `flutter.yml`
(analyze/test/web build) gates only clients/flutter changes;
`codeql.yml` does static security analysis. No cargo features, no
benches - revisit the matrix if either appears.

## Releases (`release.yml`) - VERSION BUMPS SHIP

Bumping the workspace version in Cargo.toml ON MAIN tags `vX.Y.Z` and
publishes: server+CLI binaries (linux x64/arm64, windows, macos arm64,
`mods/` bundled), Flutter desktop bundles, two all-in-one Steam-depot-
shaped archives (the linux one fits the Steam Deck), and a GHCR image.
Draft-then-publish: the release goes live only after ALL binary jobs
succeed. A `checksums` job asserts all nine archives, validates them,
writes SHA256SUMS, and best-effort keyless-signs it (cosign/Sigstore;
signing never fails the release). Keep pubspec.yaml's version in step.
Release binaries use the size/perf profile (LTO, 1 codegen unit,
strip; panic=unwind kept - one room task must unwind, not abort the
process). Auto release notes categorize by PR label; dependabot pins
actions. Current version: 0.19.0 (a 1.0.0 bump was staged once in
2026-07 and deliberately withdrawn - treat version changes as release
decisions, because they are).

## Deployment (docs/deployment.md)

Reference: `compose-deploy.yml` + `.env` (gitignored - secrets) behind
any reverse proxy, with Rauthy as the OIDC issuer (client id
`parcello`, EdDSA id tokens). Patterns: NPM + Cloudflare; parallel
community servers sharing the main identity; independent servers;
guest LAN mode. `compose-example.yml` = local build-from-source.
Operator knobs are flags/env (PARCELLO_*); client-visible defaults go
through /config.json (ADR-0032), never rebuilds. Per-IP throttling is
the proxy's job. The Dockerfile is multi-stage (checksummed Flutter
SDK build stage -> rust:1.96-slim -> bookworm-slim) and verified
end-to-end incl. the --web-dir fail-loud boot check.

## Dev environment

`.devcontainer/` ships Rust stable + pinned 1.96, Flutter + Linux
desktop toolchain, docker CLI, cargo-audit/license, typst.
Windows/macOS client artifacts are CI's job only.
