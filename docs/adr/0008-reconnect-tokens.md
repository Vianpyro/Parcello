# ADR-0008: per-seat reconnect tokens

Status: accepted

## Context
Rejoin is by identity, and guest identities are just names: anyone knowing
(or guessing) a player's name could take over their seat mid-game ("last
connection wins"). JWT identities do not have this problem - the signature
proves them - but the interim guest mode (ADR-0003) is how most rooms run
today.

## Decision
Every seat gets a reconnect token at first join: 32 alphanumeric chars
from the thread CSPRNG, returned in `Joined` (`reconnect` field, additive)
and stored on the seat for the room's lifetime. Rejoining a seat held by a
*spoofable* identity (`Identity.spoofable`, set by the verifier: guests
yes, HS256 no) requires presenting that token in `AuthPayload.reconnect`;
comparison avoids early exit (no byte-position timing signal). JWT
identities rejoin as before - cryptography already proves them.

Clients persist the token: web in `localStorage` per room code, Flutter in
`APPDATA/parcello/reconnect.json`, CLI prints it (`--reconnect` to rejoin).

## Consequences
- Seat hijacking mid-game now requires the token, not just the name.
  First-join name squatting remains possible in guest mode (documented
  ADR-0003 limitation; the Identity Service fixes it for real).
- A guest who loses the token cannot re-take their seat; the seat stays
  reserved until the room idles out. Acceptable for room-scoped games;
  revisit (host kick?) if it bites.
- Tokens are room-scoped and die with the room; nothing is persisted
  server-side.
