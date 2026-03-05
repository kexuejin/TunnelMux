# TunnelMux

![CI](https://github.com/kexuejin/TunnelMux/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/kexuejin/TunnelMux/actions/workflows/release.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)

TunnelMux is an open-source tunnel gateway and reverse-proxy control plane for local services.

If you need a production-grade replacement for ad-hoc `cloudflared` / `ngrok` scripts, TunnelMux gives you a consistent API, deterministic routing, and provider lifecycle management in one standalone service.

## Why TunnelMux

Most teams start with manual tunnel commands, then hit the same issues:
- no shared control API for CLI/GUI/automation
- fragile process supervision and log visibility
- hard-to-manage routing across multiple local services
- inconsistent failover behavior when upstreams are unstable

TunnelMux solves this by separating concerns:
- TunnelMux owns tunnels, routing, health, and operations
- your product/application acts as an API client
- no vendor-specific business adapters inside TunnelMux

## Key Capabilities

- Independent daemon (`tunnelmuxd`) and CLI (`tunnelmux-cli`)
- Tunnel lifecycle API: start, stop, status, logs, streams
- Host/path routing to multiple local upstream services
- Primary/fallback failover with active health checks
- HTTP and WebSocket proxying (`ws` and `wss` upstream support)
- Provider supervision with exponential backoff restart
- Optional token authentication for control-plane endpoints
- Multi-platform GitHub Release binaries with `SHA256SUMS`

## Installation

### 1) One-command installer (macOS/Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Examples:

```bash
# Pin a version
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --version v0.1.3

# Install into /usr/local/bin
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --prefix /usr/local
```

### 2) GitHub Releases

Download the platform package from Releases and extract:
- `tunnelmuxd`
- `tunnelmux-cli`

### 3) Install from source

```bash
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmuxd --locked
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmux-cli --locked
```

For local development:

```bash
cargo install --path crates/tunnelmuxd --force
cargo install --path crates/tunnelmux-cli --force
```

Windows users should use release `.zip` assets.

## Quick Start (60 seconds)

```bash
git clone https://github.com/kexuejin/TunnelMux.git
cd TunnelMux
cargo check

TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmuxd -- \
  --listen 127.0.0.1:4765 \
  --gateway-listen 127.0.0.1:18080 \
  --max-auto-restarts 10 \
  --health-check-interval-ms 5000 \
  --health-check-timeout-ms 2000 \
  --health-check-path /healthz

TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- routes add \
  --id app-web \
  --upstream-url http://127.0.0.1:3000 \
  --fallback-upstream-url http://127.0.0.1:3001 \
  --health-check-path /healthz \
  --host app.local

curl -H 'Host: app.local' http://127.0.0.1:18080/
```

## Architecture and Integration

- [Architecture](docs/ARCHITECTURE.md)
- [API](docs/API.md)
- [Third-Party Integration](docs/INTEGRATION.md)
- [Roadmap](docs/ROADMAP.md)
- [Releasing](docs/RELEASING.md)

## Repository Layout

- `crates/tunnelmux-core` — shared domain models and protocol types
- `crates/tunnelmuxd` — daemon runtime and control-plane API
- `crates/tunnelmux-cli` — CLI client and operational commands
- `scripts/install.sh` — release installer for macOS/Linux
- `docs/` — architecture, API, integration, roadmap, release process

## Security and Operations

- Default local binding (`127.0.0.1`) for control endpoints
- Optional bearer token (`--api-token` / `TUNNELMUX_API_TOKEN`)
- Provider logs available via API and SSE streams
- Runtime health snapshots for upstream and route observability

## Contributing

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
