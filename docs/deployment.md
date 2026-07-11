# Deploying a Parcello server

How to run a public Parcello game server with real identity checks
(EdDSA tokens from a self-hosted Rauthy, ADR-0009), behind your own
reverse proxy. Covers three setups:

1. the **main deploy** - game server + identity provider on one box;
2. a **parallel community server** that trusts the main identity
   provider (same player accounts, separate games);
3. a **fully independent server** - own identity database, or guest
   mode for LAN parties.

The moving parts are `compose-deploy.yml` + `.env` at the repo root.
TLS, domains and routing stay in your reverse proxy (this guide uses
Nginx Proxy Manager + Cloudflare); the compose file only publishes two
plain-HTTP ports.

```
players ── Cloudflare ── Nginx Proxy Manager ──┬── :7878  parcello (WebSocket + Flutter Web client)
           (DNS+proxy)   (TLS, Let's Encrypt)  └── :8080  rauthy   (OIDC issuer, admin UI)
                                                     volumes: parcello-data, rauthy-data
```

## 1. Main deploy

Prerequisites: Docker with compose v2, two DNS names (say
`game.example.com` and `auth.example.com`), NPM already running.
The server image is published by every release as
`ghcr.io/vianpyro/parcello-server` (`:latest` + `:vX.Y.Z`); the GHCR
package must be public, or `docker login ghcr.io` with a
`read:packages` token first. No release yet or deploying a fork: swap
`image:` for `build: .` in the compose file.

```sh
cp .env.example .env      # set RAUTHY_PUB_URL=auth.example.com + admin creds
docker compose -f compose-deploy.yml up -d
docker compose -f compose-deploy.yml logs -f   # watch first boot
```

Expected in the parcello logs: `EdDSA identity provider enabled` then
`JWKS refreshed`. The JWKS fetch retries every cycle, so the order the
containers come up in never matters.

### Nginx Proxy Manager

Two proxy hosts, both `http` scheme, target = the docker host's
address (or the container names, if you attached the stack to NPM's
network - see the comment at the bottom of `compose-deploy.yml`):

| Domain | Forward to | Notes |
| --- | --- | --- |
| `game.example.com` | `:7878` | **WebSockets Support: ON** (required). |
| `auth.example.com` | `:8080` | Rauthy admin + OIDC endpoints. |

On both: request a Let's Encrypt certificate, Force SSL, HTTP/2.
Rauthy runs with `PROXY_MODE=true` and must see your proxy's address
in `TRUSTED_PROXIES` (default: all private ranges; tighten it in
`.env`).

### Cloudflare

- Two A/CNAME records, proxied (orange cloud) is fine - Cloudflare
  passes WebSockets on every plan.
- SSL/TLS mode: **Full (strict)** (NPM has a real certificate).
- Idle games can outlive proxy idle timers; if you see silent
  disconnects mid-lobby, that is the CF/NPM idle timeout, not a crash -
  reconnection reattaches the seat (rejoin by identity + reconnect
  token, ADR-0008).

### Create the OIDC client for the game

Rauthy first boot: log into `https://auth.example.com/auth/v1/admin`
with the bootstrap credentials from `.env` (or `admin@localhost` + the
random password from `docker compose logs rauthy` if you left them
unset). Then create the client the game clients will log in through:

1. Clients -> New client, id `parcello` (must equal
   `PARCELLO_IDENTITY_AUDIENCE`).
2. Public client (no secret), flow `authorization_code`, **PKCE S256
   required** - the Flutter client (desktop and web) is a public client.
3. Redirect URIs - register both, the desktop and web builds use
   different flows from the same client id:
   - `http://127.0.0.1:*` - desktop (`oidc_login_io.dart`) opens the
     system browser and listens on an ephemeral loopback port
     (RFC 8252), so the port must stay a wildcard. If your Rauthy
     version refuses the wildcard, pin a fixed port and register it
     exactly.
   - `https://game.example.com/oidc-callback.html` (your own origin) -
     web (`oidc_login_web.dart`) opens a popup to this static page,
     which forwards the authorization response back via `postMessage`
     and closes itself; a browser page can't bind a loopback port, so
     it can't reuse the desktop flow.
4. Token signing: set the ID token algorithm to **EdDSA** (Ed25519) -
   the server only verifies EdDSA (ADR-0009).
5. Scopes: the client requests `openid profile`; the defaults cover it.
   Display names come from `name`/`preferred_username`, falling back
   to `sub`.

Then have players Register (or create their accounts in the admin UI,
if you keep registration closed). Test: Flutter client (desktop or
`wss://game.example.com` in a browser) -> Login; only the CLI accepts a
pasted token instead.

### Day 2

- **Update**: `docker compose -f compose-deploy.yml pull && docker
  compose -f compose-deploy.yml up -d`. Pin `PARCELLO_TAG` to a release
  tag if you prefer explicit upgrades.
- **Backup**: stop the stack, copy the `parcello-data` (game history
  SQLite, ADR-0005) and `rauthy-data` (accounts) volumes, start again.
  Both are plain files; cold copies are the safe ones.
- **Never** set `PARCELLO_INSECURE_GUEST` on a WAN-facing server:
  guest identities are spoofable by design (ADR-0003). The deprecated
  HS256 path (`PARCELLO_JWT_SECRET`) also stays out of deployments.
- No admin control plane by design - moderation happens in Rauthy
  (accounts), not in the game server. The server needs no infrastructure
  beyond its own binary plus two runtime-resolved sibling directories,
  same idea as `mods/`: `web/` (the Flutter Web build, served at `/`;
  `--web-dir`/`PARCELLO_WEB_DIR`, ADR-0025 - already bundled by the
  published Docker image and release tarballs) and `mods/`. Health probe
  at `/healthz`.

## 2. Parallel community server (shared accounts)

A second game server, anywhere, that accepts the SAME player accounts
because it trusts the main Rauthy - grant this only to operators you
trust to run a server in your network's name:

```yaml
# compose.yml on the second box - no Rauthy here.
services:
  parcello:
    image: ghcr.io/vianpyro/parcello-server:latest
    restart: unless-stopped
    environment:
      PARCELLO_IDENTITY_URLS: https://auth.example.com/auth/v1/oidc/certs
      PARCELLO_IDENTITY_AUDIENCE: parcello
      PARCELLO_HISTORY: data/parcello.db
    volumes:
      - parcello-data:/srv/parcello/data
    ports:
      - "7878:7878"
volumes:
  parcello-data:
```

What is shared and what is not - this is the Minecraft model, not a
cluster:

- **Shared: identity.** One account works on every server that points
  at the same JWKS; the main Rauthy operator controls registrations
  and bans for all of them.
- **Not shared: everything else.** Rooms live in the memory of one
  server (they dissolve after 30 idle minutes) and history is a local
  SQLite file per server. There is no game-state replication or room
  synchronisation; "redundancy" means players can join another server
  with the same account when one is down, never that a running game
  fails over.
- `PARCELLO_IDENTITY_URLS` takes a comma list for redundant instances
  of the SAME issuer (same keys) - it is not a way to trust two
  different identity databases at once.

## 3. Fully independent server

- **Own identity**: run the full `compose-deploy.yml` with your own
  domains and your own `RAUTHY_PUB_URL` - your Rauthy, your user
  database, your rules. Nothing links your server to anyone else's.
- **LAN party / no accounts**:

  ```sh
  docker run --rm -p 7878:7878 ghcr.io/vianpyro/parcello-server \
    --insecure-guest
  ```

  Guests pick a display name and play; identities are spoofable, so
  keep this strictly on trusted networks. Native binaries from the
  GitHub release work the same (`parcello-server --insecure-guest
  --lan` also announces itself for LAN discovery, ADR-0016 - the
  container cannot do multicast, use the binary for that).

References: [Rauthy documentation](https://sebadob.github.io/rauthy/)
([config reference](https://sebadob.github.io/rauthy/config/config.html)),
[RFC 8252 (OAuth for native apps)](https://www.rfc-editor.org/rfc/rfc8252.html).
