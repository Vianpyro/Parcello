# Protocol fixtures

Golden wire-format fixtures for the network protocol (protocol duplication
audit, Strategy S1: `docs/protocol-duplication-audit.md`). Each file is a
JSON object keyed by variant name:

- `command_kind.json` - every `CommandKind` variant (`crates/engine/src/command.rs`)
- `event.json` - every `Event` variant (`crates/engine/src/event.rs`)
- `command_error.json` - every `CommandError` variant (`crates/engine/src/error.rs`)
- `client_message.json` - every `ClientMessage` variant (`crates/protocol/src/lib.rs`)
- `server_message.json` - the `ServerMessage` variants that don't embed
  `ClientView`/`ResolvedContent` (`Joined`, `Spectating`, `GameStarted`,
  `Update` are deferred to S2 - see the doc comment in
  `crates/protocol/tests/protocol_fixtures.rs`)

## Why these exist

The Flutter client can't import the Rust types, so it hand-mirrors these
enums in Dart (`describeEvent`, `rejectReason`, and the ad hoc `Map`
literals built for outgoing commands in `clients/flutter/lib/`). Nothing
used to catch that mirror drifting from the real Rust types, and a
protocol-only change didn't even trigger the Flutter CI job. These fixtures
are the shared ground truth both sides test against, so a forgotten Dart
case now fails loudly instead of silently falling into a `default:`.

## They are the canonical wire format

Every value in these files is exactly what `serde_json` produces for the
corresponding Rust enum variant - not a hand-written approximation. The
Rust tests (`crates/{engine,protocol}/tests/protocol_fixtures.rs`) assert
this both ways: the checked-in JSON deserializes back to the same value,
and the value re-serializes to exactly the same JSON. The Dart tests
(`clients/flutter/test/protocol_fixtures_test.dart`) load the same files
and check that `describeEvent`/`rejectReason` handle every entry, and that
every `CommandKind`/`ClientMessage`/`ServerMessage` tag is on a known
allowlist.

## Never hand-edit these files

Editing a fixture by hand only makes it agree with itself, not with what
Rust actually serializes - the whole point is that these come *from* the
real types, not from a contributor's memory of the wire format. Manually
"fixing" a failing fixture defeats the test.

## How to regenerate

1. Add or change the Rust variant as usual.
2. Add a match arm in the corresponding `*_name`/`*_fixtures` function(s) in
   `crates/engine/tests/protocol_fixtures.rs` or
   `crates/protocol/tests/protocol_fixtures.rs` (the match has no wildcard
   arm, so the compiler forces this).
3. Regenerate the fixture file(s) from those canonical instances:

   ```sh
   cargo test -p parcello-engine --test protocol_fixtures -- --ignored regenerate_fixtures
   cargo test -p parcello-protocol --test protocol_fixtures -- --ignored regenerate_fixtures
   ```

4. Commit the updated fixture file(s).
5. Update the Dart side that needs to know about the new variant
   (`describeEvent`/`rejectReason`, or the allowlist `Set` in
   `protocol_fixtures_test.dart`) - the new Dart test fails explicitly
   until you do.
