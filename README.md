# TunnelMux

[English](README.md) | [简体中文](README.zh-CN.md)

![CI](https://github.com/kexuejin/TunnelMux/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/kexuejin/TunnelMux/actions/workflows/release.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)

TunnelMux is a GUI-first local tunnel control console for developers who are tired of juggling `cloudflared`, `ngrok`, route scripts, and half-broken local demos.

If your project now means “frontend + API + docs + callback endpoint” instead of one localhost port, TunnelMux gives you one place to start tunnels, expose services, switch providers, and see what is actually broken.

![TunnelMux GUI home screen](docs/images/gui-home.png)

## Why people reach for TunnelMux

Modern local sharing gets messy fast:

- vibe coding turns one app into multiple local services in a day
- ad-hoc `cloudflared` / `ngrok` commands become tribal knowledge
- path and host routing drifts across scripts, shell history, and README snippets
- when something fails, it is hard to tell whether the problem is the daemon, the tunnel, the route, or the local service
- teammates cannot reliably reproduce the same local exposure setup

TunnelMux keeps that workflow in one local control plane instead of another pile of terminal glue.

## What you get

- A desktop GUI for the common path: create a tunnel, click start, add services
- One daemon and one API behind both the GUI and CLI
- Multi-service host/path routing for local apps, APIs, docs, and callbacks
- Provider-aware tunnel setup for `cloudflared` and `ngrok`
- Runtime status, public URL, and service state in one place
- Route health, provider logs, and diagnostics when you need them
- Declarative `config.json` hot reload for route and health-check changes

## GUI-first workflow

TunnelMux is designed for the “I just need this working” path first:

1. Create a tunnel profile
2. Pick `cloudflared` or `ngrok`
3. Click `Start Tunnel`
4. Add one or more local services
5. Share the public URL

When you need more control, the same app also supports:

- multiple tunnel profiles
- provider-specific configuration
- tunnel-scoped services
- tunnel restart / recovery
- diagnostics and log inspection on demand

## Install

### Fastest path: native GUI installer

Download the latest installer from GitHub Releases:

- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

Releases also include raw platform archives with:

- `tunnelmuxd`
- `tunnelmux-cli`
- `tunnelmux-gui`

### One-command installer

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Examples:

```bash
# Pin a version
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --version v0.2.0

# Install into /usr/local/bin
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --prefix /usr/local
```

### Build from source

```bash
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmuxd --locked
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmux-cli --locked
```

For local development:

```bash
cargo run -p tunnelmuxd
cargo run -p tunnelmux-gui
```

## Quick start

### GUI path

1. Install `cloudflared` or `ngrok`
2. Open TunnelMux
3. Create your first tunnel
4. Click `Start Tunnel`
5. Add your local service URL, for example `http://127.0.0.1:3000`

The GUI prefers to connect to an existing local `tunnelmuxd`. If nothing is reachable, it can auto-start a local daemon for the desktop app.

If the selected provider is not installed yet, TunnelMux now catches that before launch, shows a provider-specific warning on the main page, and offers a `Copy Install Command` action for the current tunnel instead of surfacing a raw spawn error.

### CLI path

```bash
git clone https://github.com/kexuejin/TunnelMux.git
cd TunnelMux

cargo run -p tunnelmuxd -- \
  --listen 127.0.0.1:4765 \
  --gateway-listen 127.0.0.1:18080

cargo run -p tunnelmux-cli -- routes add \
  --id app-web \
  --upstream-url http://127.0.0.1:3000 \
  --path-prefix /app

cargo run -p tunnelmux-cli -- tunnel start \
  --provider cloudflared \
  --target-url http://127.0.0.1:18080 \
  --auto-restart
```

## Supported local workflow

TunnelMux is a good fit when you need to expose:

- a frontend on one path and an API on another
- docs, webhook callbacks, and local tools behind one tunnel
- a stable named Cloudflare tunnel or a quick temporary tunnel
- one tunnel today, then multiple tunnel profiles later

It is not trying to be your production edge or cloud platform. It is the local control layer that makes local sharing less annoying.

## macOS first-launch FAQ

Current native GUI installers may still be unsigned, so macOS can show Gatekeeper warnings on first launch.

### “TunnelMux is damaged and can’t be opened”

If you trust the download source:

1. Open Finder and locate the app
2. Right-click `TunnelMux.app`
3. Click `Open`
4. Confirm the trust prompt

If macOS still blocks it, go to:

- `System Settings` → `Privacy & Security`
- find the blocked app notice near the bottom
- click `Open Anyway`

### “Apple cannot verify the developer”

Use the same sequence first:

1. Right-click the app
2. Click `Open`
3. Confirm the dialog

If needed:

- `System Settings` → `Privacy & Security`
- click `Open Anyway`

### Last resort: remove quarantine

Only do this if you trust the source of the app:

```bash
xattr -dr com.apple.quarantine /Applications/TunnelMux.app
```

More release and bundle details live in `docs/RELEASING.md`.

## Config files

- `~/.tunnelmux/config.json` — declarative routes and health-check settings
- `~/.tunnelmux/state.json` — daemon-owned runtime snapshot

The daemon polls `config.json` and applies route and health-check changes without restarting.

## Docs

- [Architecture](docs/ARCHITECTURE.md)
- [API](docs/API.md)
- [Third-Party Integration](docs/INTEGRATION.md)
- [Integration Templates](docs/INTEGRATION-TEMPLATES.md)
- [Roadmap](docs/ROADMAP.md)
- [Releasing](docs/RELEASING.md)
- [Changelog](CHANGELOG.md)

## Repository layout

- `crates/tunnelmux-core` — shared domain models and protocol types
- `crates/tunnelmux-control-client` — shared HTTP control client for CLI and GUI
- `crates/tunnelmuxd` — daemon runtime and control-plane API
- `crates/tunnelmux-cli` — CLI client and operational commands
- `crates/tunnelmux-gui` — Tauri desktop control console
- `scripts/install.sh` — installer for macOS/Linux

## Contributing

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
