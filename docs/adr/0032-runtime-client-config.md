# ADR-0032: Runtime client config via `/config.json`

## Context

The server serves the Flutter Web build from disk at runtime (ADR-0025).
Some client defaults are deployment-specific - first among them the OIDC
issuer URL a hosted player should see pre-filled in the sign-in dialog. Baking
that value with `--dart-define` at `flutter build web` time works for a
source-built client, but it is a *compile-time* value: it cannot be changed on
the published GHCR image without rebuilding the whole web bundle. That is the
opposite of how the deployment is operated - `compose-deploy.yml` pulls a
prebuilt image and an operator (Ansible, in the reference homelab) sets plain
environment variables. There was no runtime channel from an operator's env to
a client default.

## Decision

The server exposes `GET /config.json`, a small JSON document of per-deployment
client defaults, added to `game_router` (so the WebSocket integration tests
cover it too). The first field is `default_issuer`, fed by
`--default-issuer` / `PARCELLO_DEFAULT_ISSUER`. Unset fields are omitted from
the JSON; the web client reads the document once at startup and applies any
value it recognises, otherwise keeping its own compile-time default.

Precedence for the sign-in issuer field, most specific first: a player's
remembered provider (`savedIssuer`) > the server's `/config.json` value > the
`--dart-define` compile-time default > the bare `https://` scheme. The fetch
is web-only (a desktop client connects to arbitrary servers, so there is no
single serving origin to ask) and best-effort (a missing or broken
`/config.json` is silently ignored - the default is a convenience, never
required).

## Consequences

- An operator sets the pre-filled issuer through a normal compose env var on
  the prebuilt image; no rebuild, which is what an Ansible-driven homelab
  wants. The `--dart-define` build knob stays for source-built and desktop
  clients.
- `/config.json` is a public, unauthenticated endpoint. It must only ever
  carry values that are already public and safe to hand any visitor (a
  provider URL is). It is a config surface, not a secrets channel - do not put
  anything sensitive behind it.
- New per-deployment client defaults now have a home: add a field to
  `ClientConfig`, plumb a flag, read it on the client. This is additive - the
  omit-when-unset shape keeps old clients and new servers (and vice versa)
  compatible.
