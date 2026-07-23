#!/usr/bin/env bash
# Print the project's release version: the workspace `version` in the root
# Cargo.toml, the SINGLE source of truth for the whole project (it names the
# release tag, the binaries, and the server --version).
#
# This is the ONE extraction point. Every place that needs the version reuses
# it instead of re-implementing the parse: release.yml names the tag from it,
# and every Flutter build injects it with
# `--dart-define=PARCELLO_VERSION="$(cargo_version.sh)"` so the client reads it
# back as a compile-time constant (lib/version.dart) - the same transport as the
# git SHA. pubspec.yaml carries only a fixed placeholder Flutter requires; it is
# NOT a version source, so there is nothing to keep in sync and nothing to drift.
#
# Usage: clients/flutter/tool/cargo_version.sh
#   Cargo.toml is resolved at the repo root relative to this script, or from
#   $PARCELLO_CARGO_TOML when set (the Docker build stage has no repo root).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CARGO_TOML="${PARCELLO_CARGO_TOML:-$SCRIPT_DIR/../../../Cargo.toml}"

[ -f "$CARGO_TOML" ] || { echo "error: Cargo.toml not found at '$CARGO_TOML' (set PARCELLO_CARGO_TOML)" >&2; exit 2; }

# The workspace package version: the first line-anchored `version = "..."`.
# Dependency versions are `name = { version = ... }` (not line-anchored), so
# this matches only the package version.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$CARGO_TOML" | head -n1)"
[ -n "$version" ] || { echo "error: no workspace version in '$CARGO_TOML'" >&2; exit 2; }

printf '%s\n' "$version"
