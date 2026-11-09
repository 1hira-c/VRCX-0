# Desktop-agent boundary (design; agent not implemented)

The Web/HAOS process owns the VRChat API session, cookies, SQLite profile, feed,
friends, favorites, search and maintenance. Browsers are views of this shared
runtime. Closing a browser must never stop collection. A desktop agent is a
separate device connected to this server, not a second database owner or a copy
of the user's VRChat credentials.

The current HTTP adapter has an explicit command allowlist in
`crates/web/commands.txt`. It does not expose process launch, arbitrary file reads,
registry writes or desktop IPC. `/api/capabilities` identifies the server and
returns `desktopAgents: { supported: false, devices: [] }`. Existing UI host
capabilities mark device operations unavailable, with an agent-specific reason.
Do not make these permanently unavailable merely because the UI is a browser.

## Planned agent protocol v1

- The agent initiates an authenticated outbound WSS connection to the server.
  This permits HAOS and desktop machines behind NAT without inbound desktop ports.
- Pairing uses a short-lived, single-use code approved in the Web UI. Issue a
  revocable device credential, separate from the Web access token and VRChat
  credentials. Persist the device identity, allowed capabilities and last-seen
  time. Never trust a browser-supplied device identity as authorization.
- The device advertises versioned capabilities: `game.logs.read`,
  `game.process.observe`, `game.launch`, `screenshots.read`, `osc.send`,
  `notifications.show`, `vr.overlay`. A device being paired does not authorize
  every capability. The user selects a target when several devices are online.
- Capability resolution combines server, browser and selected-agent capabilities.
  The browser may open links/copy text locally; game/OS operations go only to the
  selected device. The HAOS server must not fall back to executing them locally.
- Upload game-log records with `{deviceId, streamId, sequence, observedAt, payload}`.
  Store a durable acknowledgement cursor and deduplicate retries transactionally.
  Resume after outages and rotations. Associate observations with the verified
  VRChat account and device; do not merge unrelated PC sessions into one instance.
- Device actions use a typed, allowlisted envelope with `requestId`, `deviceId`,
  capability, parameters and expiry. Launch uses a validated VRChat location and
  an agent-configured executable, never a remote shell command. Never replay a
  queued launch after reconnect. Show cancellation, expiry and offline failures.
- Screenshots are uploaded/downloaded as bounded blobs or opaque IDs. Local paths
  remain on the originating device. Limit file roots, size and content types.
  Web browsers must not receive an endpoint for reading arbitrary server paths.
- Disconnect/revocation immediately removes available device capabilities and
  rejects new actions. Heartbeats, bounded queues and per-device rate limits
  prevent a slow/offline device from blocking recording or other users.

## Implementation order and acceptance checks

1. Pair/revoke/list devices and show capability/online status; test revoked and
   wrong-device credentials, expiry and protocol-version mismatch.
2. Reliable game-log ingest; test deduplication, reconnect, rotation and conflicting
   accounts on two PCs. Reuse existing Rust game-log ingestion contracts.
3. Presence and notifications, then launch/OSC/overlays and screenshot transfers.
   Test offline action expiry, explicit target selection and local capability consent.

The existing Integration API is a room-state stream, not this device-control
protocol. It must not silently become an unauthenticated command channel.
