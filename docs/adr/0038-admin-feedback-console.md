# ADR-0038: admin-only feedback console

Status: accepted

## Context

The post-game survey (`feedback` message) has recorded ratings and comments
since it landed, but nothing ever reads them back: `GameHistory` is a
write-only port (`record_feedback`, history.rs) and the server's only HTTP
surface is `/healthz`, `/config.json`, `/ws` plus the Flutter Web static
service. The answers exist in SQLite and are invisible without a `sqlite3`
prompt.

A *public* feedback wall with upvotes and reports was considered and
rejected:

- The table is per server. Each community host owns its own SQLite, so the
  voting population would be one server's players, not the game's. Cross-
  server aggregation needs signed results - ADR-0009's stats note, and
  `docs/security-model.md`'s rule "never build a feature that requires
  trusting a community server". A host can fabricate accounts and votes.
- Sample sizes are small enough that a vote score ranks noise.
- Publishing a survey changes what people write. The value of the current
  data is that it is addressed to the maintainer at the end of a game;
  a voted, public feed rewards performance and buries negative feedback,
  which is the most useful kind.

What is needed is far smaller: let the operator read their own server's
answers, in the context of the game that produced each one.

## Decision

**A read-only console at `/admin`, authorised per server, not per issuer.**

**Authorisation is an operator-set allowlist of token subjects**
(`--admin <sub>`, repeatable, plus `PARCELLO_ADMIN_SUBS` as a comma or
whitespace separated list). Not an issuer-side role or group claim, even
though Rauthy can carry one, because `docs/deployment.md` explicitly
supports parallel community servers **sharing one identity provider**: a
`parcello_admin` role in the token would make its holder an administrator
of every server on that issuer, including other people's. Trust boundary 4
of `docs/security-model.md` ("operator flags are trusted") is the correct
one for this decision. The cost is that an operator must copy a `sub` once.

Three consequences follow, all enforced in `admin.rs`:

1. **An empty allowlist disables the whole surface, answering 404** - not
   403. A server with no `--admin` has no console and does not advertise
   one. This is the default, so the new surface is opt-in.
2. **Spoofable identities are never administrators.** Guest tokens
   (ADR-0003/0008) are refused before the allowlist is consulted, the same
   rule the ranked queue applies (ADR-0034). An unforgeable identity is the
   price of reading other players' words.
3. **The credential is the ordinary id token**, verified by the existing
   `IdentityVerifier` (so `exp`, `aud` and the JWKS signature checks are the
   ones already audited), carried as `Authorization: Bearer`. No session,
   no cookie, no new token type.

**Reads go through a new port, `FeedbackQuery`, not `GameHistory`.** Same
reasoning that split `RatingStore` out in ADR-0034: `GameHistory` is
append-only and fire-and-forget with a writer thread that owns its
connection (ADR-0005), and `MemoryHistory` cannot answer a relational
query at all. `SqliteFeedbackQuery` opens its own **read-only** connection
on the same database file; WAL is already enabled, so readers never block
the writer. Queries run under `spawn_blocking`.

**One query, no pagination**: the most recent `FEEDBACK_QUERY_LIMIT` = 500
rows of `feedback` joined to `game`, filtered and sorted in the page. At the
volumes this data actually reaches, server-side paging would be ceremony,
and the whole point of the console is to see the set at once.

**The page is a single self-contained HTML file compiled into the binary**
(`include_str!`), served at `/admin`. It is deliberately not part of the
Flutter bundle (ADR-0025 serves that from disk at runtime): an admin screen
in the client would ship admin code to every player and spend the client's
Brotli size budget (`web-perf.yml`), and the console needs none of the game
client's machinery. Being compiled in means it is always present and needs
no deployment step - the opposite trade-off from ADR-0025's multi-megabyte,
independently-rebuilt bundle, and appropriate at one file.

The page authenticates with an ordinary PKCE authorization-code redirect
(not the client's popup flow, `oidc_login_web.dart` - a full-page redirect
has no popup-blocker failure mode), reading the issuer from `/config.json`
and using the same `parcello` client id as the game client. It stores the
PKCE verifier in `sessionStorage` and keeps the token in memory only.

## Consequences

- The operator registers `<origin>/admin` as a redirect URI on the OIDC
  client, alongside the client's loopback wildcard (`docs/deployment.md`).
- Without `--history` the console has nothing to read and says so; it does
  not disappear, because a blank page would be indistinguishable from a
  broken one.
- Feedback rows carry the author's `player` id, which for token identities
  is the opaque `sub` and for guests is their chosen name. It reaches
  administrators only, and the page shows it truncated; the console never
  displays it to anyone else because there is no anyone else.
- `GET /admin` itself is unauthenticated. It is an empty shell: every datum
  arrives from `/admin/api/feedback`, which is not.
- A future write action (deleting an abusive comment, exporting) fits the
  same guard; adding one is a new decision, not an extension of this one.
- Reversing this ADR means deleting `admin.rs`, `feedback.rs`, the asset and
  the flag. Nothing else depends on them.
