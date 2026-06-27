# m-webshell

**The keep for your web terminal.**

A web terminal that sits quietly behind a time-based one-time password, with no sessions, no cookies, and no exposed ports. Invalid or expired codes get a bare `404` — indistinguishable from a missing page. There is no login form, no `401`, no `403`, and no indication that a terminal even exists.

---

## The Problem

You need remote shell access to a server through a web browser — for debugging, emergency recovery, or headless machines with no SSH client handy. A web terminal gives you a full shell from any browser, anywhere.

**But a web terminal with no authentication is a root shell waiting to be found.** If you expose it on a public port, anyone who discovers the URL gets full access to your machine.

Common workarounds all have serious drawbacks:

| Approach | Problem |
|----------|---------|
| HTTP Basic Auth | Credentials are static — once leaked, the server is compromised forever. No second factor. |
| nginx `auth_request` + external TOTP script | Requires a Perl module or shell-out to `oathtool` on every request. Fragile, hard to deploy, and the TOTP code appears in nginx logs. |
| VPN / SSH tunnel | Requires client-side setup. Not accessible from a random browser. Overkill for a single terminal. |
| Cloudflare Access / OAuth proxy | Adds an external dependency and a SaaS account. Overkill for a single admin terminal. |
| Just don't expose it | Then you can't use it when you actually need it. |

What you actually want: **type a 6-digit code into the URL bar, get a terminal for 30 seconds, and have it silently disappear when the code expires.**

---

## The Solution

**m-webshell** is a small Rust daemon that:

1. **Manages an internal terminal engine** bound to a **Unix domain socket** — never exposed on any TCP port.
2. **Validates a TOTP code from the URL path** on every single request (HTTP and WebSocket).
3. **Strips the TOTP prefix** and proxies the rest to the terminal engine over the Unix socket.
4. **Returns a bare `404`** (no body, no text) for any invalid, expired, or missing code — indistinguishable from a page that doesn't exist.
5. **Rate-limits** all requests to prevent brute-force guessing.
6. **Supports file transfer** — upload and download files directly in the browser terminal.

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
             │ (internal, not exposed)
             ▼
┌─────────────────────────┐
│   Terminal engine        │
│   /bin/login             │
│   (no TCP port exposed)  │
└─────────────────────────┘
```

### Key security properties

- **No exposed ports** — the terminal engine only listens on a Unix socket; m-webshell only listens on `127.0.0.1`.
- **No sessions** — TOTP is re-validated on every request. When the code expires, access stops instantly.
- **No information leakage** — All failures return bare `404` with zero body. The front-end proxy intercepts these and shows its own generic 404 page. No `401`, no `403`, no error text that hints a terminal exists.
- **No TOTP in logs** — Paths are normalized to `/system/manage-******` before logging.
- **Rate limiting** — Global token-bucket limiter (default: 30 requests/minute) prevents brute-force attacks.
- **Strict 30-second validation** — Only the current TOTP step is valid. Previous and next steps are rejected. No clock-skew tolerance.
- **Unprivileged shell** — the terminal engine drops to a specified UID before spawning `/bin/login`.
- **No external binaries for auth** — TOTP validation is pure Rust. No shell-out to `oathtool`.

---

## Installation

### Prerequisites

- A Linux server with root access (Debian/Ubuntu)

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
git clone https://github.com/mufkuw/m-webshell.git
cd m-webshell
cargo build --release

sudo install -Dm755 target/release/m-webshell /usr/local/bin/m-webshell
sudo install -Dm644 m-webshell.service /etc/systemd/system/m-webshell.service
sudo systemctl daemon-reload
sudo systemctl enable m-webshell
```

### Post-install: scan the QR code

After installation, a secret is generated automatically. To view it:

```bash
m-webshell show-secret
```

This prints a QR code in your terminal. Scan it with Google Authenticator, Authy, 1Password, or any TOTP app.

### Configuration

All options can be set via CLI flags or environment variables:

| Flag | Env var | Default | Description |
|------|---------|---------|-------------|
| `--listen` | `TTG_LISTEN` | `127.0.0.1:12479` | Address to bind the gate server |
| `--uid` | `TTG_TTYD_UID` | **required** | UID to drop the login shell to |
| `--secret-file` | `TTG_SECRET_FILE` | `/etc/m-webshell/m-webshell.totp` | Path to the base32 TOTP secret |
| `--rate` | `TTG_RATE` | `30/minute` | Rate limit (e.g. `10/second`, `100/hour`) |
| `--title` | `TTG_TITLE` | `m-webshell` | Browser page title for the terminal |
| `--font-size` | `TTG_FONT_SIZE` | `16` | Terminal font size in pixels |

### systemd service

The included `m-webshell.service` runs the daemon as root (needed to spawn the login shell), with hardening options enabled:

```ini
[Unit]
Description=m-webshell — the keep for your web terminal
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/m-webshell --ttyd-uid 1000 serve
Restart=always
RestartSec=5
User=root
ProtectSystem=strict
ProtectHome=no
ReadWritePaths=/run/m-webshell /home
PrivateTmp=true
StateDirectory=m-webshell
RuntimeDirectory=m-webshell
RuntimeDirectoryMode=0750

Environment=TTG_LISTEN=127.0.0.1:12479
Environment=TTG_SECRET_FILE=/etc/m-webshell/m-webshell.totp
Environment=TTG_RATE=30/minute
Environment=TTG_TITLE=m-webshell
Environment=TTG_FONT_SIZE=16

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

### File transfer

You can upload and download files directly in the browser terminal:

- **Download**: Type `sz filename` in the terminal — the browser will save the file.
- **Upload**: Drag and drop files into the browser window, or use the upload button in the terminal toolbar.

Requires `lrzsz` installed on the server (`apt install lrzsz`).

### Regenerating the secret

If you need to rotate the TOTP secret:

```bash
m-webshell generate-secret
systemctl restart m-webshell
```

This generates a new secret, saves it, and prints a fresh QR code to scan.

---

## How it works (request flow)

### Valid TOTP, HTTP request

```
Browser → nginx → m-webshell:12479
  Path: /system/manage-652857/
  m-webshell extracts "652857"
  m-webshell validates against current TOTP step → valid
  m-webshell strips prefix → path becomes "/"
  m-webshell proxies to terminal engine via Unix socket
  Terminal page returned
  Browser renders the terminal
```

### Valid TOTP, WebSocket upgrade

```
Browser → nginx → m-webshell:12479
  Path: /system/manage-652857/ws
  m-webshell validates TOTP → valid
  m-webshell strips prefix → path becomes "/ws"
  m-webshell upgrades the client WebSocket
  m-webshell opens a WebSocket to the terminal engine via Unix socket
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
  No connection to the terminal engine is ever attempted
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

---

## Project structure

```
src/
  main.rs          # Entrypoint: parse CLI, spawn terminal engine, start server
  lib.rs           # Re-exports all modules
  cli.rs           # CLI parser with env-var backing
  config.rs        # Resolved runtime config
  gate.rs          # Request handler: TOTP check, path rewrite, HTTP proxy
  ws.rs            # WebSocket upgrade + bidirectional bridge
  totp.rs          # TOTP verification (RFC 6238, strict 30s window)
  ratelimit.rs     # Rate limiter (global token bucket)
  backend.rs       # Terminal engine child process management
  show_secret.rs   # "show-secret" and "generate-secret" subcommands (QR code)
tests/
  integration_tests.rs  # HTTP proxy, 404 cases, path normalization
```

---

## Security notes

- The TOTP secret file should be `chmod 640` and owned by `root:root`.
- `m-webshell` binds to `127.0.0.1` by default — it is **not** directly accessible from the network. All traffic must go through a front-end proxy.
- TOTP validation is strict 30-second — only the current step is accepted, no skew tolerance.
- The login shell requires a valid username/password. TOTP is the first factor; the system login is the second.
- The rate limiter is global (not per-IP). This is intentional — it limits total request volume to the gate.

---

## License

MIT