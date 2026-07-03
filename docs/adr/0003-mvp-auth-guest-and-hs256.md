# ADR-0003: MVP auth = guest mode and optional HS256, behind a trait

Status: accepted (interim)

## Context
The Global Identity Service (OAuth -> global JWT) does not exist yet. Rooms
still need stable player identities for seating and rejoin.

## Decision
`IdentityVerifier` trait in the session layer. MVP implementations:
- `--insecure-guest`: display name = identity (spoofable, LAN/testing only);
- `PARCELLO_JWT_SECRET`: HS256 JWT verification, implemented with
  hmac/sha2/base64 (pure Rust, no `ring` build dependency).

## Consequences
The future Identity Service (asymmetric keys, JWKS fetch) is a new
`IdentityVerifier` implementation; transport and room code do not change.
Shared-secret HS256 is a stopgap: it does not scale to community-hosted
servers and is not a trust boundary.
