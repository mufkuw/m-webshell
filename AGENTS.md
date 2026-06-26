# Agent Notes for m-webshell

## Build & test

```
cargo build              # debug
cargo build --release    # release (LTO enabled)
cargo test               # all tests (unit + integration)
cargo clippy             # lint
cargo fmt                # formatting
```

Integration tests require `python3` in PATH (starts `tests/mock_ttyd.py` as a child). The mock has a 1.5s startup delay — tests sleep after spawning.

## Single-crate project

```
src/
  main.rs          # entrypoint
  lib.rs           # re-exports modules
  cli.rs           # Clap CLI parser with env-var backing
  config.rs        # Resolved runtime config
  gate.rs          # Request handler: TOTP check, path rewrite, HTTP proxy
  ws.rs            # WebSocket upgrade + bidirectional bridge
  totp.rs          # TOTP verification (strict 30s, no skew)
  ratelimit.rs     # Rate limiter (governor, global bucket, per-instance)
  backend.rs       # Backend (terminal engine) child process management
  show_secret.rs   # "show-secret" subcommand (URI + QR)
tests/
  integration_tests.rs  # HTTP proxy, path rewrite, 404 cases
  mock_ttyd.py          # Python3 mock upstream for integration tests
```

## Key quirks not obvious from source

- **Rate limiter is global, not per-IP.** `check_key` ignores the IP argument and uses a single `governor::NotKeyed` bucket. The `_ip: &IpAddr` parameter is unused.
- **Unix socket path is communicated via env var.** `gate::UnixConnector` reads `_TTG_TTYD_SOCKET` from the environment (set by `main.rs` at startup from config), not from `AppState`.
- **Backend CLI arg.** The actual command passes `--interface <socket_path>`. See `src/backend.rs:21-24`.
- **TOTP validation is strict (no skew).** `check()` only validates the exact current 30-second step. No window/tolerance.
- **RFC test vector sanity check runs only in debug builds.** `src/totp.rs:37-39`: at `t=59` with the known secret, `generate(59)` must equal `"287082"`. Silently logged as error if it mismatches (doesn't panic).
- **Backend spawn failure is non-fatal.** `main.rs:65-69`: if the backend can't start, the server continues running. Integration tests rely on this (they use `mock_ttyd.py` instead).
- **No CI, no pre-commit config.**

## Security invariants (enforced by design, verified from source)

- Every auth failure → 404 (never 401/403): `src/gate.rs:93,98,103`
- TOTP codes are never logged: `normalize_path()` replaces digits with `******` before logging
- The terminal backend binds to a Unix socket only (no TCP port)
- `show-secret` subcommand prints the provisioning URI + ANSI QR to stdout

## Test coverage

| Area | Where | Form |
|------|-------|------|
| TOTP validation | `src/totp.rs` | Unit test with RFC 6238 fixed-time vector |
| Rate-limit parsing | `src/ratelimit.rs` | Unit tests for various rate strings |
| HTTP proxy | `tests/integration_tests.rs` | Integration with python3 mock, verifies path rewrite + 200 |
| Missing prefix → 404 | `tests/integration_tests.rs` | Integration with mock |
| Path normalization | `tests/integration_tests.rs` | Unit test of `normalize_path()` |

## CLI / env map

All options are dual-sourced via `clap` `env` attribute. See `src/cli.rs` for defaults — they match the systemd unit in `m-webshell.service`.
