# m-webshell — Design Document

## Goal

Build a single, self-contained Rust daemon that:
1. Validates a TOTP embedded in the URL path.
2. Strips the TOTP prefix.
3. Proxies HTTP and WebSocket traffic to an internally-spawned `ttyd` child process.
4. Returns 404 for any invalid/missing TOTP.

This lets any front-end HTTP server (nginx, Apache, Caddy, Traefik, etc.) expose a secure web terminal by simply proxying `/manage-*` to the daemon.

## Why this approach

| Concern | Current nginx+Perl | Rust daemon |
|---------|---------------------|-------------|
| External TOTP binary | `oathtool` shell-out | `totp-rs` in-process |
| WebSocket proxy complexity | nginx rewrite + auth_request quirks | Native Rust WS proxy |
| Portability | nginx + Perl module required | Any HTTP server can proxy |
| Deployability | Multiple config files + secret mgmt | One systemd unit |
| Attack surface | nginx Perl, shell open3 | No shell, no Perl module |

## Architecture

```text
┌──────────────┐       /manage-<TOTP>/...        ┌──────────────────────┐
│   Client     │ ──────────────────────────────▶ │   Front-end server   │
└──────────────┘                                   │  (nginx, Caddy, ...) │
                                                   └──────────┬───────────┘
                                                              │ proxy_pass
                                                              ▼
                                                   ┌──────────────────────┐
                                                   │  m-webshell      │
                                                   │  127.0.0.1:12479     │
                                                   │                      │
                                                   │  • extract TOTP      │
                                                   │  • validate          │
                                                   │  • strip prefix      │
                                                   │  • proxy to ttyd     │
                                                   └──────────┬───────────┘
                                                              │ Unix socket or loopback
                                                              ▼
                                                   ┌──────────────────────┐
                                                   │  ttyd child process  │
                                                   │  /bin/login          │
                                                   └──────────────────────┘
```

## Components

### 1. Rust binary `m-webshell`

Responsibilities:
- Listen on a TCP socket (default `127.0.0.1:12479`).
- Read the base32 TOTP secret from a file at startup.
- Spawn `ttyd` as a child process on startup, bound to a Unix domain socket.
- For every request:
  - Match path against `/manage-([0-9]{6})(/.*)?`.
  - Validate the 6-digit code with `totp-rs`.
  - On failure: return 404 immediately.
  - On success: rewrite path to `/\2` (or `/` when no trailing path), then proxy to `ttyd`.
- WebSocket upgrades are forwarded transparently.
- Rate limit per source IP.
- Structured logging without ever logging the TOTP code.
- Provide a CLI subcommand to generate and display the TOTP provisioning URI and QR code.

### 2. ttyd child process

Started and managed by the daemon:
```bash
/usr/local/bin/ttyd --interface 127.0.0.1 --port 0 \
                    --socket /run/m-webshell/ttyd.sock \
                    -W -o -m 1 -u <uid> /bin/login
```

Notes:
- `ttyd` listens on a Unix socket owned by the gate process only.
- The gate owns the child and reaps it on shutdown.
- `ttyd` runs the actual login prompt as an unprivileged user.

### 3. Front-end server snippet

Example nginx:
```nginx
location ~ "^/manage-[0-9]{6}" {
    proxy_pass http://127.0.0.1:12479;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # No timeout on terminal sessions
    proxy_read_timeout 86400s;
    proxy_send_timeout 86400s;
}
```

No TOTP logic, no Perl, no `auth_request`.

## Crate Selection

| Need | Crate | Reason |
|------|-------|--------|
| HTTP server + routing | `axum` | Mature, ergonomic, tokio-native |
| WebSocket server + proxy | `tokio-tungstenite` | Already used by axum's ws feature; we need it for the upstream connection too |
| TOTP generation/validation | `totp-rs` | Pure Rust, RFC-6238, configurable skew |
| Async runtime | `tokio` (full) | Process spawning, networking, WS |
| Process management | `tokio::process` | Async child process |
| Rate limiting | `governor` or simple in-memory token bucket | Per-IP throttling |
| Configuration | `clap` + env vars | CLI flags and environment |
| Logging | `tracing` + `tracing-subscriber` | Structured, no TOTP leakage |
| Base32 secret decoding | `base32` (via `totp-rs`) | Standard decoding |

## Request Flow

### HTTP request (e.g. page load)

1. nginx receives `GET /manage-123456`.
2. nginx proxies to `127.0.0.1:12479` with the same path.
3. Gate matches regex, extracts `123456`, validates with `totp-rs`.
4. If valid, rewrite path to `/` and forward request body/headers to ttyd's Unix socket.
5. Forward ttyd's response back to nginx.

### WebSocket upgrade (e.g. `/manage-123456/ws?token=...`)

1. nginx proxies the upgrade request to the gate.
2. Gate validates TOTP from the prefix.
3. Gate strips the prefix and initiates its own WebSocket client connection to ttyd.
4. Gate bridges the two WebSocket streams bidirectionally until either side closes.

### Bad TOTP

1. Gate matches path but validation fails.
2. Return `404 Not Found` with no body (or a minimal nginx-like HTML body).
3. No connection to ttyd is ever attempted.

## Configuration

CLI / environment options:

| Option | Env | Default | Description |
|--------|-----|---------|-------------|
| `--listen` | `TTG_LISTEN` | `127.0.0.1:12479` | Gate HTTP listen address |
| `--ttyd-socket` | `TTG_TTYD_SOCKET` | `/run/m-webshell/ttyd.sock` | Unix socket for ttyd child |
| `--ttyd-bin` | `TTG_TTYD_BIN` | `/usr/local/bin/ttyd` | Path to ttyd binary |
| `--ttyd-uid` | `TTG_TTYD_UID` | required | UID to run login shell as |
| `--secret-file` | `TTG_SECRET_FILE` | `/etc/totp-secret` | Base32 TOTP secret |
| `--rate` | `TTG_RATE` | `30/minute` | Per-IP rate limit |
| `--totp-window` | `TTG_TOTP_WINDOW` | `1` | Allowed 30s steps of skew |

### CLI subcommands

| Subcommand | Purpose |
|------------|---------|
| `serve` (default) | Run the gate daemon |
| `show-secret` | Print the provisioning URI and an ANSI QR code |

Example:
```bash
m-webshell show-secret --secret-file /etc/totp-secret
# otpauth://totp/ttyd:server?secret=...
# [ANSI QR code]
```

## Security Model

1. **TOTP as the only auth**: no cookies, no sessions. Every request is checked.
2. **ttyd not network-reachable**: it only listens on a Unix socket created by the gate.
3. **Shell runs unprivileged**: `ttyd` starts as root (to call `/bin/login`), but the spawned shell uses `-u <uid>`.
4. **No secret in logs**: the gate logs normalized paths like `/manage-******/token`.
5. **No shell injection**: TOTP validation is pure Rust; no external binary is called.
6. **Rate limiting**: per-IP token bucket; over-limit returns 404.
7. **Bind to localhost**: default listen is `127.0.0.1` so the front-end server is required.

## Error Handling

| Scenario | Response |
|----------|----------|
| Path missing `/manage-` prefix | 404 |
| TOTP wrong length or non-numeric | 404 |
| TOTP doesn't match current window | 404 |
| Rate limit exceeded | 404 |
| ttyd child not running | 503 |
| Internal panic | 500 (rare) |

## Testing Strategy

1. Unit test TOTP validation using a known secret and time.
2. Integration test HTTP proxy to a mock upstream.
3. Integration test WebSocket bridging with `tokio_tungstenite`.
4. End-to-end test with real ttyd in a container.
5. Playwright browser test: page load, token fetch, WS connect, refresh after TOTP expiry.

## Deployment

1. Build release binary.
2. Install to `/usr/local/bin/m-webshell`.
3. Create systemd service.
4. Configure front-end server to proxy `/manage-*` to the gate.
5. Ensure `/etc/totp-secret` is `root:<gate-user>` 640.

## Future Enhancements (not in scope)

- mTLS between front-end server and gate.
- Prometheus metrics.
- One-time TOTP tracking (track used codes to prevent replay within the same window).
- Multiple allowed secrets / hot-reload of secret.
- Audit log of successful/failed attempts to syslog.

## Why not embed ttyd entirely?

Embedding a PTY and terminal emulation in Rust would mean reimplementing a large part of ttyd/xterm.js. Spawning the official `ttyd` binary keeps the terminal logic battle-tested and lets us focus only on the auth gate.
