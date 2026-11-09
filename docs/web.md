# VRCX-0 Web / HAOS (experimental)

The existing React UI runs against a Rust HTTP host. Tauri IPC is adapted at the
platform boundary; the existing typed data/API commands, VRChat login (including
2FA/device approval), data persistence and runtime events are reused. The HTTP
host shares the data services currently housed in runtime-host-desktop, without
creating a Tauri window or starting its local desktop/game services. Further
extraction of these data services into a host-neutral crate is possible later.

## Run locally

Requires Node >=24.10 and Rust >=1.99. From the repository root:

```sh
npm ci
npm run build:web
cargo +stable run -p vrcx-0-web -- --data-dir /absolute/path/to/vrcx-web-data
```

Open `http://127.0.0.1:8080/`. Enter the access token from the first startup log,
then sign into VRChat in the existing UI. A browser reconnect restores runtime
snapshots; the server keeps running when all browsers close. A server restart
attempts to restore the saved VRChat session. An expired session can be renewed
through the UI. This is one shared profile, not a multi-user hosting service.

For development, leave the Rust server running and use `npm run dev:web`. Vite
proxies `/api` to the Rust server. Desktop builds retain their existing behavior.

## Docker

```sh
docker compose up --build -d
docker compose logs vrcx
```

The default published address is loopback. For remote use, place a TLS reverse
proxy in front, forward streaming responses without buffering, and set
`VRCX_WEB_SECURE_COOKIE=1`. Do not publish raw HTTP to the Internet. Static assets
and API URLs are relative so an ingress path works with the existing hash router.

- `VRCX_WEB_LISTEN`: default `127.0.0.1:8080` (container: `0.0.0.0:8080`).
- `VRCX_WEB_STATIC_DIR`: built UI directory (default `dist`).
- `VRCX_WEB_TOKEN_FILE`: optional existing token file, at least 32 characters.
- `VRCX_WEB_SECURE_COOKIE=1`: require HTTPS for the HttpOnly Web session cookie.
- `--data-dir`: persistent profile directory. Container: `/data/profile`.

Back up the complete profile while stopped, including `web-secret-key`. This
key is stable across container replacements; losing it loses access to encrypted
VRChat credentials. `web-access-token` authorizes Web login; deleting it while
stopped rotates access on the next start. Web sessions expire after 30 days and
are invalidated on restart. Protect profile backups as credentials.

## HAOS

Version 0.1.0 images for amd64 and aarch64 are published on GHCR and can be pulled
without authentication. See [add-on instructions](../home-assistant/vrcx-0/DOCS.md).

For maintainers: pushing a tag matching
`web-v<add-on version>` (initially `web-v0.1.0`) runs the Web Image workflow and
publishes native amd64/aarch64 images. The workflow rejects mismatched versions.
Make both GHCR packages public before installation. Manual dispatch is also
available once the workflow definition exists on the default branch. Packaging follows the official
[Home Assistant configuration reference](https://contributors.home-assistant.io/apps/reference/configuration/).

## Scope

Core data/API commands are explicitly allowlisted in `crates/web/commands.txt`;
unsupported commands return an error. Server startup/database snapshots are
adapted separately. Desktop IPC, local file selection/uploads, screenshot tools,
OSC, game launch and overlays are not available yet. The old VRCX Web snapshot
upload/import workflow is not ported. Do not copy a legacy VRCX database over the
VRCX-0 database. Some optional settings/tools still expose unavailable actions.

[Desktop-agent design](desktop-agent.md) preserves the intended server/device
split and specifies pairing, capability selection, reliable log ingest and
bounded actions. An agent has not been implemented in this change.
