# 0.1.2

- Let the Web server own automatic sign-in; browser sign-in waits for startup to finish.
- Resume services from database-runtime readiness, including recovery outside the HTTP retry route.
- Support “Start with new database”: archive the old database, restart the server runtime, and reconnect the browser.
- Wait for active migrations and disconnected requests before stopping services.
- Release upgrade waiters after a panic in unwind builds and allow another recovery attempt.

# 0.1.1

- Resume background services and automatic sign-in after a successful database-upgrade retry, without restarting the server.
- Keep failed or interrupted upgrades waiting for recovery; repeated successful requests do not restart the backend.

# 0.1.0

- Initial experimental Web host and Home Assistant Ingress packaging.
- Browser authentication, VRChat login/2FA, shared runtime events, persistent data and encryption key.
- Explicit server capabilities and desktop-agent extension plan.
