# ADR-0033: Chosen in-game handle for authenticated players

## Context

Before this change, a token-authenticated player's public name came entirely
from their identity provider (ADR-0009): the display name broadcast to the
table was whatever `safe_display_name` derived from the token's `name` /
`preferred_username` / `sub` claims. That has two problems. First, players
could not pick a game handle - they were stuck with their account name (often
a real name, which many do not want shown to strangers at a game table).
Second, the connect screen's display-name field did nothing once signed in:
the client sent either a `token` or a `guest_name`, never both, and the server
ignored any name when a token was present - an input that silently had no
effect.

Guests, by contrast, already choose their shown name (`guest_name`), which is
also their identity. Authenticated players had strictly less control over a
purely cosmetic value.

## Decision

`AuthPayload` gains an optional `display_name`. When a **token** is present,
the server verifies the token for identity as before - `player_id` stays
derived from the `sub`, unchanged - and then, if `display_name` is present and
survives sanitisation, uses it as the public `name`. The identity and the
displayed name are now separate: the token proves *who you are*, the handle is
*what the table calls you*.

`sanitize_display_name` gives the chosen handle the same hardening as any
broadcast text (it is shown to every seat): control characters and Unicode
bidi/zero-width format characters are stripped (no display-spoofing or
log-injection - the same `is_unsafe_format` set as `sanitize_comment`), inner
whitespace is collapsed, and it is capped at the 24-char name budget. A handle
containing `@` is rejected wholesale, so the email guard of ADR-0009 still
holds even through the override path. Crucially, an invalid or empty handle is
*ignored* (the token's own safe name is kept) rather than failing the login -
a cosmetic field must never block authentication.

The client seeds the field with the account's name (`jwtDisplayName`) as the
default handle on sign-in without clobbering anything the player typed, and
sends `display_name` alongside the `token`. Guests are unchanged: their
`guest_name` already is their display name, so `display_name` is ignored for
them.

## Consequences

- The connect-screen name field is meaningful in every mode: the guest
  identity when not signed in, the chosen handle when signed in (defaulting to
  the account name). The dead input is gone.
- Identity is unaffected: reconnection, seat ownership, and history keys all
  key on `player_id` (the token `sub`), not the mutable display name.
- Authenticated handles are now arbitrary, like guest names already were - the
  same moderation surface, no worse. Names remain bounded and stripped of
  spoofing characters; moderation of accounts stays in the identity provider
  (Rauthy), consistent with "no admin control plane" (docs/deployment.md).
- The wire gains one optional, omit-when-unset field; old clients (no
  `display_name`) and new servers, and vice versa, stay compatible.
