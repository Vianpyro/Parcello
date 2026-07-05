# ADR-0016: LAN discovery multicast address and protocol

Status: accepted

## Context

The server implements a LAN discovery mechanism so local clients can
find game hosts without manual URL entry. Early development used the
well-known Minecraft discovery group/address (`224.0.2.60:4445`) as a
reference. That address and port are effectively claimed by other
applications and pose a risk of cross-application packet collisions and
unexpected firewall treatment.

Multicast addressing has well-defined allocation ranges. For robust
local discovery we must avoid colliding with known services and pick an
administratively-scoped address and a high ephemeral port.

## Decision

- Use an administratively-scoped multicast address in the `239/8`
  range by default. The initial choice is `239.255.0.1`.
- Use a high, ephemeral default port `55888` to reduce the chance of
  firewall or application collisions.
- Announcements carry a small JSON payload with explicit identification
  fields so clients can safely ignore unrelated multicast traffic. The
  format (documented here) is:

```json
{
  "app": "parcello",
  "proto": "parcello-discovery-v1",
  "bind": "0.0.0.0:7878",
  "ts": 1688530000
}
```

- Discovery is opt-in: `--lan` enables the announcer (off by default).
  `--lan-maddr`, `--lan-port`, and `--lan-broadcast-fallback` override the
  defaults at startup.
- Discovery is announce-only; the server exposes no remote admin/control
  plane. Managing a local server (start/stop/restart) is the client's job
  via the OS process, not an HTTP endpoint on the public game port.
- The server optionally also sends the same payload to the global
  broadcast address `255.255.255.255:<port>` when `--lan-broadcast-fallback`
  is supplied to improve discovery on networks where multicast is
  restricted or unsupported.

## Alternatives considered

- Reusing `224.0.2.60:4445` (Minecraft). Rejected due to likely
  collisions and poor etiquette.
- Use broadcast-only (255.255.255.255). Rejected as primary mechanism
  because broadcast may be filtered and lacks the scoping advantages
  of administratively-scoped multicast. Kept as an optional fallback.
- Use mDNS / DNS-SD (Bonjour). This provides richer discovery but
  adds complexity and platform-specific behavior; consider in future
  if richer naming and service discovery are required.

## Consequences

- Clients must filter incoming discovery packets by `proto` (and
  `app` optionally) to avoid acting on unrelated multicast traffic.
- Network operators can reconfigure the multicast address and port via
  server flags to avoid local conflicts; the defaults should work in
  most home/office networks without impacting well-known services.
- Firewalls and NAT devices may still block multicast or the chosen
  port; the broadcast fallback and configurability mitigate this.
- The discovery protocol is simple and replay-safe: the `bind` field
  is authoritative for the connection target and does not expose any
  internal server state.

## Migration / Future work

- Consider advertising richer metadata (`players`, `max_players`,
  `mod_id`, `server_version`) in the payload once the wire format is
  stable.
- Consider MDNS / DNS-SD integration for networks where multicast is
  unreliable or to provide human-friendly names.
- Document the discovery protocol in `docs/protocol.md` if more
  clients/platforms adopt it.
