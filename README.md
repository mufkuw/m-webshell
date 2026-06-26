# m-webshell

**The keep for your web terminal.**

A web terminal that sits quietly behind a time-based one-time password, with no sessions, no cookies, and no exposed ports. Invalid or expired codes get a bare `404` — indistinguishable from a missing page. There is no login form, no `401`, no `403`, and no indication that a terminal even exists.

---

## The Problem

You need remote shell access to a server through a web browser — for debugging, emergency recovery, or headless machines with no SSH client handy. The standard tool for this is [`ttyd`](https://github.com/tsl0922/ttyd), which serves a full terminal emulator over HTTP/WebSocket.

**ttyd by itself has no authentication.** If you expose it on a public port, anyone who finds the URL gets a root shell on your machine.

Common workarounds all have serious drawbacks:

| Approach | Problem |
|----------|---------|
| HTTP Basic Auth over ttyd | Credentials are static — once leaked, the server is compromised forever. No second factor. |
| nginx `auth_request` + external TOTP script | Requires a Perl module or shell-out to `oathtool` on every request. Fragile, hard to deploy, and the TOTP code appears in nginx logs. |
| VPN / SSH tunnel | Requires client-side setup. Not accessible from a random browser. Overkill for a single terminal. |
| Cloudflare Access / OAuth proxy | Adds an external dependency and a SaaS account. Overkill for a single admin terminal. |
| Just don't expose it | Then you can't use it when you actually need it. |

What you actually want: **type a 6-digit code into the URL bar, get a terminal for 30 seconds, and have it silently disappear when the code expires.**

---

## The Solution

**m-webshell** is a small Rust daemon that:

1. **Spawns `ttyd` as a child process**, bound to a **Unix domain socket** — never exposed on any TCP port.
2. **Validates a TOTP code from the URL path** on every single request (HTTP and WebSocket).
3. **Strips the TOTP prefix** and proxies the rest to `ttyd` over the Unix socket.
4. **Returns a bare `404`** (no body, no text) for any invalid, expired, or missing code — indistinguishable from a page that doesn't exist.
5. **Rate-limits** all requests to prevent brute-force guessing.

There are no sessions, no cookies, no tokens, and no state between requests. The TOTP code is checked fresh on every HTTP request and every WebSocket upgrade. When the 30-second window expires, the terminal stops working immediately.

### Architecture

```
  Browser
     │
     │  HTTPS (via Cloudflare / nginx / Caddy / Apache)
     ▼
┌─────────────────────────┐
│   Front-end proxy       │
│   (nginx, Caddy, etc.)  │
│   port 80/443           │
└────────────┬────────────┘
             │ proxy_pass to 127.0.0.1:12479
             ▼
┌─────────────────────────┐
│   m-webshell             │
│   127.0.0.1:12479        │
│                          │
│   • Extract TOTP from    │
│     /system/manage-XXXX/ │
│   • Validate (30s window)│
│   • Strip prefix         │
│   • Rate limit           │
│   • 404 on any failure   │
└────────────┬────────────┘
             │ Unix domain socket
             │ /run/m-webshell/ttyd.sock
             ▼
┌─────────────────────────┐
│   ttyd (child process)  │
│   /bin/login            │
│   (no TCP port exposed) │
└─────────────────────────┘
```

### Key security properties

- **No exposed ports** — `ttyd` only listens on a Unix socket; `m-webshell` only listens on `127.0.0.1`.
- **No sessions** — TOTP is re-validated on every request. When the code expires, access stops instantly.
- **No information leakage** — All failures return bare `404` with zero body. The front-end proxy intercepts these and shows its own generic 404 page. No `401`, no `403`, no error text that hints a terminal exists.
- **No TOTP in logs** — Paths are normalized to `/system/manage-******` before logging.
- **Rate limiting** — Global token-bucket limiter (default: 30 requests/minute) prevents brute-force attacks.
- **Strict 30-second window** — Only the current TOTP step is valid (window = 0). Previous and next steps are rejected.
- **Unprivileged shell** — `ttyd` drops to a specified UID before spawning `/bin/login`.
- **No external binaries for auth** — TOTP validation is pure Rust (`totp-rs` crate). No shell-out to `oathtool`.

---

## Installation

### Prerequisites

- A Linux server with root access (Debian/Ubuntu)
- `ttyd` binary installed (`/usr/local/bin/ttyd`) — the `.deb` package lists it as a dependency

### Option A: One-line install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/mufkuw/m-webshell/main/scripts/install.sh | bash
```

This downloads the latest `.deb` package from GitHub releases and installs it via `dpkg`. It also:
- Creates `/etc/m-webshell/` and generates a random TOTP secret
- Installs the systemd service file
- Enables the service (you start it after setting the UID)

### Option B: Install the .deb manually

Download the latest `.deb` from [GitHub releases](https://github.com/mufkuw/m-webshell/releases) and install:

```bash
sudo dpkg -i m-webshell_*_amd64.deb
sudo apt-get install -f   # fix any missing dependencies
```

### Option C: Build from source

```bash
# Clone and build
git clone https://github.com/mufkuw/m-webshell.git
cd m-webshell
cargo build --release

# Install the binary
sudo install -Dm755 target/release/m-webshell /usr/local/bin/m-webshell

# Create config directory and generate secret
sudo mkdir -p /etc/m-webshell
openssl rand -base32 24 | tr -d '=' | tr '/+' '_-' | head -c 32 | sudo tee /etc/m-webshell/m-webshell.totp
sudo chmod 640 /etc/m-webshell/m-webshell.totp

# Install the systemd service
sudo install -Dm644 m-webshell.service /etc/systemd/system/m-webshell.service

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable --now m-webshell
```

### Option B: Manual build

```bash
cargo build --release
cp target/release/m-webshell /usr/local/bin/
```

### Post-install: scan the QR code

```bash
m-webshell show-secret
```

This prints the `otpauth://` provisioning URI and an ANSI QR code in your terminal. Scan it with Google Authenticator, Authy, 1Password, or any TOTP app.

### Configuration

All options can be set via CLI flags or environment variables:

| Flag | Env var | Default | Description |
|------|---------|---------|-------------|
| `--listen` | `TTG_LISTEN` | `127.0.0.1:12479` | Address to bind the gate server |
| `--ttyd-socket` | `TTG_TTYD_SOCKET` | `/run/m-webshell/ttyd.sock` | Unix socket path for ttyd |
| `--ttyd-bin` | `TTG_TTYD_BIN` | `/usr/local/bin/ttyd` | Path to the ttyd binary |
| `--ttyd-uid` | `TTG_TTYD_UID` | **required** | UID to drop the login shell to |
| `--secret-file` | `TTG_SECRET_FILE` | `/etc/m-webshell/m-webshell.totp` | Path to the base32 TOTP secret |
| `--rate` | `TTG_RATE` | `30/minute` | Rate limit (e.g. `10/second`, `100/hour`) |
| `--totp-window` | `TTG_TOTP_WINDOW` | `0` | Allowed 30s steps of skew (0 = strict) |

### systemd service

The included `m-webshell.service` runs the daemon as root (needed to spawn `ttyd` with `/bin/login`), with hardening options enabled:

```ini
[Unit]
Description=m-webshell TOTP auth gate daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/m-webshell serve --ttyd-uid 1000
Restart=always
RestartSec=5
User=root
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/run/m-webshell
PrivateTmp=true
StateDirectory=m-webshell
RuntimeDirectory=m-webshell
RuntimeDirectoryMode=0750

Environment=TTG_LISTEN=127.0.0.1:12479
Environment=TTG_TTYD_SOCKET=/run/m-webshell/ttyd.sock
Environment=TTG_TTYD_BIN=/usr/local/bin/ttyd
Environment=TTG_SECRET_FILE=/etc/m-webshell/m-webshell.totp
Environment=TTG_RATE=30/minute
Environment=TTG_TOTP_WINDOW=0

[Install]
WantedBy=multi-user.target
```

Adjust `--ttyd-uid` to the UID of the user you want the shell to run as.

### Front-end proxy configuration

See **[EXAMPLES.md](EXAMPLES.md)** for ready-to-use configurations for nginx, Caddy, and Apache.

---

## Usage

1. Open your TOTP app and get the current 6-digit code.
2. Navigate to `https://your-domain.com/system/manage-<CODE>/`
3. You get a terminal.
4. The code expires in 30 seconds. After that, any further requests (page loads, WebSocket, assets) return `404`.

**That's it.** No login page, no cookie, no session. Type the code in the URL, use the terminal, done.

### Regenerating the secret

If you need to rotate the TOTP secret:

```bash
# Generate a new secret
openssl rand -base32 24 | tr -d '=' | tr '/+' '_-' | head -c 32 > /etc/m-webshell/m-webshell.totp

# Restart the daemon
systemctl restart m-webshell

# Scan the new QR code
m-webshell show-secret
```

---

## How it works (request flow)

### Valid TOTP, HTTP request

```
Browser → nginx → m-webshell:12479
  Path: /system/manage-652857/
  m-webshell extracts "652857"
  m-webshell validates against current TOTP step → valid
  m-webshell strips prefix → path becomes "/"
  m-webshell proxies to ttyd via Unix socket
  ttyd returns the terminal HTML page
  Browser renders the terminal
```

### Valid TOTP, WebSocket upgrade

```
Browser → nginx → m-webshell:12479
  Path: /system/manage-652857/ws
  m-webshell validates TOTP → valid
  m-webshell strips prefix → path becomes "/ws"
  m-webshell upgrades the client WebSocket
  m-webshell opens a WebSocket to ttyd via Unix socket
  m-webshell bridges messages bidirectionally
  Terminal is live
```

### Invalid or expired TOTP

```
Browser → nginx → m-webshell:12479
  Path: /system/manage-000000/
  m-webshell validates against current TOTP step → invalid
  m-webshell returns 404 (no body)
  nginx intercepts 404, shows generic 404 page
  No connection to ttyd is ever attempted
```

---

## Build from source

```bash
git clone https://github.com/mufkuw/m-webshell.git
cd m-webshell

# Debug build
cargo build

# Release build (with LTO)
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy

# Format
cargo fmt
```

Integration tests require `python3` in PATH (spawns a mock ttyd).

---

## Project structure

```
src/
  main.rs          # Entrypoint: parse CLI, spawn ttyd, start axum server
  lib.rs           # Re-exports all modules
  cli.rs           # Clap CLI parser with env-var backing
  config.rs        # Resolved runtime config
  gate.rs          # Request handler: TOTP check, path rewrite, HTTP proxy, UnixConnector
  ws.rs            # WebSocket upgrade + bidirectional bridge to ttyd
  totp.rs          # TOTP verification (RFC 6238, strict 30s window)
  ratelimit.rs     # Rate limiter (governor, global token bucket)
  ttyd.rs          # ttyd child process management
  show_secret.rs   # "show-secret" subcommand (prints URI + ANSI QR)
tests/
  integration_tests.rs  # HTTP proxy, 404 cases, path normalization
  mock_ttyd.py          # Python mock upstream for integration tests
```

---

## Security notes

- The TOTP secret file should be `chmod 640` and owned by `root:root`.
- `m-webshell` binds to `127.0.0.1` by default — it is **not** directly accessible from the network. All traffic must go through a front-end proxy.
- The `--totp-window` defaults to `0` (strict 30-second validation). Set it to `1` if you have clock skew between your TOTP app and the server (accepts ±30 seconds).
- `ttyd` runs `/bin/login` which requires a valid username/password. TOTP is the first factor; the system login is the second.
- The rate limiter is global (not per-IP). This is intentional — it limits total request volume to the gate.

---

## License

MIT