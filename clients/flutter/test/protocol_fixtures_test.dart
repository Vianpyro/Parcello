// Golden wire-format fixtures shared with the Rust side (protocol
// duplication audit, Strategy S1: docs/protocol-duplication-audit.md).
//
// The fixtures live in `protocol-fixtures/` at the repo root: one JSON
// object per enum (`event.json`, `command_error.json`, `command_kind.json`,
// `client_message.json`, `server_message.json`), keyed by variant name,
// generated from the real Rust types by `cargo test -p parcello-engine
// --test protocol_fixtures -- --ignored regenerate_fixtures` (and the
// `parcello-protocol` equivalent). This file is the Dart half of the same
// guarantee: every fixture entry must be decoded without silently falling
// into a `default:` case.
//
// `describeEvent` and `rejectReason` (protocol.dart) are pure switches over
// the wire tag, so "handled explicitly" is checked by asserting the result
// differs from their documented fallback (the raw event dump / the raw
// error code). `ClientMessage`/`CommandKind`/`ServerMessage` have no such
// switch to probe (Dart only ever *builds* the former two, and the
// `ServerMessage` envelope switch in session.dart is tightly coupled to
// live connection state) - for those, this file keeps its own explicit
// allowlist of known tags per type, so an unrecognised tag fails the test
// outright instead of being ignored. Adding a new wire variant therefore
// requires touching this allowlist, which is the point.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations_en.dart';
import 'package:parcello_client/protocol.dart';

/// Fixture files live two directories up from `clients/flutter` (repo root).
Map<String, dynamic> _loadFixtures(String name) {
  final file = File('../../protocol-fixtures/$name.json');
  final decoded = jsonDecode(file.readAsStringSync());
  return (decoded as Map<String, dynamic>).cast<String, dynamic>();
}

void main() {
  final loc = AppLocalizationsEn();
  String p(int i) => 'P$i';
  String t(int i) => 'T$i';

  test('every event fixture is described, none hits the silent fallback',
      () {
    final fixtures = _loadFixtures('event');
    expect(fixtures, isNotEmpty);
    for (final entry in fixtures.entries) {
      final e = (entry.value as Map<String, dynamic>);
      final described = describeEvent(e, loc, p, t);
      expect(
        described,
        isNot(equals(e.toString())),
        reason:
            'event "${entry.key}": describeEvent fell back to the raw '
            'dump - add a case to describeEvent',
      );
    }
  });

  test(
      'every command_error fixture is localized, none hits the silent '
      'fallback', () {
    final fixtures = _loadFixtures('command_error');
    expect(fixtures, isNotEmpty);
    for (final entry in fixtures.entries) {
      final code = (entry.value as Map<String, dynamic>)['code'] as String;
      final reason = rejectReason(loc, code);
      expect(
        reason,
        isNot(equals(code)),
        reason:
            'command_error "${entry.key}": rejectReason fell back to the '
            'raw code "$code" - add a case to rejectReason',
      );
    }
  });

  // CommandKind is Dart-encoded only (built as Map literals across the UI,
  // never decoded) - this allowlist is what stands in for "Dart knows about
  // this variant" absent a shared decoder.
  const knownCommandKinds = {
    'play_movement_card', 'build', 'propose_trade', 'accept_trade',
    'decline_trade', 'cancel_trade', 'submit_blind_bid', 'sell_house',
    'expropriate', 'boost_rent', 'mortgage', 'unmortgage',
    'choose_legal_route', 'offer_bribe', 'vote_on_bribe', 'use_jail_card',
    'end_turn', 'resign', //
  };

  const knownClientMessages = {
    'create', 'join', 'spectate', 'add_bot', 'remove_bot', 'configure',
    'start', 'play_again', 'leave', 'cmd', 'feedback', 'animation_done',
    'list_mods', 'queue_ranked', 'cancel_queue', 'get_rating', 'ping', //
  };

  // The subset of ServerMessage decoded by session.dart's `_handle` switch
  // that carries no ClientView/ResolvedContent payload (Joined, Spectating,
  // GameStarted, Update are deferred - see the module doc and
  // docs/protocol-duplication-audit.md, S2).
  const knownServerMessages = {
    'room_created', 'lobby', 'rejected', 'error', 'mods', 'queued',
    'match_found', 'rating', 'ratings_updated', 'pong', //
  };

  void checkTagAllowlist(String fixtureFile, Set<String> known) {
    test('every $fixtureFile fixture entry has a known tag', () {
      final fixtures = _loadFixtures(fixtureFile);
      expect(fixtures, isNotEmpty);
      for (final entry in fixtures.entries) {
        final tag = (entry.value as Map<String, dynamic>)['type'] as String;
        expect(
          known.contains(tag),
          isTrue,
          reason: '$fixtureFile entry "${entry.key}": unrecognised tag '
              '"$tag" - it must be added to the Dart-side switch(es) that '
              'build/consume it, and to the allowlist in '
              'protocol_fixtures_test.dart',
        );
      }
    });
  }

  checkTagAllowlist('command_kind', knownCommandKinds);
  checkTagAllowlist('client_message', knownClientMessages);
  checkTagAllowlist('server_message', knownServerMessages);
}
