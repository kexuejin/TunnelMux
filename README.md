# TunnelMux

[English](README.md) | [简体中文](README.zh-CN.md)

![CI](https://github.com/kexuejin/TunnelMux/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/kexuejin/TunnelMux/actions/workflows/release.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Release version](https://img.shields.io/github/v/release/kexuejin/TunnelMux?sort=semver)
![GitHub stars](https://img.shields.io/github/stars/kexuejin/TunnelMux)
![Downloads](https://img.shields.io/github/downloads/kexuejin/TunnelMux/total)

> ⭐ **If TunnelMux saves you time, star the repo — it helps more developers find it.**

**Latest release:** [v0.4.0](https://github.com/kexuejin/TunnelMux/releases/tag/v0.4.0) · [中文发布说明](docs/releases/v0.4.0.md)

TunnelMux is a GUI-first local tunnel control console for developers who are tired of juggling `cloudflared`, `ngrok`, route scripts, and half-broken local demos.

If your project now means “frontend + API + docs + callback endpoint” instead of one localhost port, TunnelMux gives you one place to start tunnels, expose services, switch providers, and see what is actually broken.

**Plays great with local AI tooling** — mount DeepSeek Harness, Ollama, or Open WebUI behind one clickable path, with built-in access gates so public routes stay protected.

![TunnelMux desktop console](docs/images/gui-home.png)

## What's new in v0.4.0

- **One desktop runtime**: the app hosts the daemon in-process, owns one control port and provider process tree, and adopts an already-running daemon without stopping it.
- **Rebuilt console**: Overview, Tunnels, Services, Diagnostics, and Settings views in a persistent sidebar, with semantic Auto / Dark / Light themes.
- **Tunnel-scoped control plane**: routes, access gates, logs, health, metrics, and CLI operations consistently target the selected tunnel instead of silently sharing `primary` state.
- **Safer local secrets**: GUI control and provider tokens move to macOS Keychain, Windows Credential Manager, or the Linux Secret Service integration; `settings.json` keeps only non-secret configuration.
- **Fail-closed updater**: verified `.tar.gz` and `.zip` raw archives, mandatory SHA-256, safe archive handling, and a clear manual-update path for native `.dmg` / `.msi` / `.deb` installs.

See the [v0.4.0 release notes](docs/releases/v0.4.0.md) and [changelog](CHANGELOG.md) for the full scope.

## Contents

- [What's new in v0.4.0](#whats-new-in-v040)
- [Why TunnelMux](#why-people-reach-for-tunnelmux)
- [What you get](#what-you-get)
- [Install](#install)
- [Quick start](#quick-start)
- [Config and credentials](#config-and-credentials)
- [Service access gates](#service-access-gates)
- [Security](#security)
- [macOS first-launch FAQ](#macos-first-launch-faq)
- [FAQ](#faq)
- [Docs](#docs)
- [Repository layout](#repository-layout)
- [Contributing](#contributing)

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
- One in-process daemon and one HTTP control plane behind both the GUI and CLI
- Five focused views — Overview, Tunnels, Services, Diagnostics, and Settings — instead of one long settings page
- Auto / Dark / Light appearance plus English / Simplified Chinese UI switching
- Multi-service host/path routing for local apps, APIs, docs, and callbacks
- Provider-aware tunnel setup for `cloudflared` and `ngrok`, including in-app install guidance
- Tunnel-scoped routes, access gates, logs, health checks, metrics, and CLI operations
- Route health, provider logs, streaming diagnostics, and recovery guidance when you need them
- Service access gates with a global default code plus per-service inherit/custom/public modes
- OS credential storage for GUI control and provider tokens
- In-app update checks against GitHub Releases with mandatory SHA-256 verification for `.tar.gz` and `.zip` raw archives
- Declarative `config.json` hot reload for route and health-check changes

## GUI-first workflow

TunnelMux is designed for the “I just need this working” path first:

1. Create a tunnel profile
2. Pick `cloudflared` or `ngrok`
3. Click `Start Tunnel`
4. Add one or more local services
5. Share the public URL

When you need more control, the same app also supports:

- a persistent sidebar for Overview, Tunnels, Services, Diagnostics, and Settings
- UI language and appearance selection in Settings → Interface; Auto follows the system language and theme
- multiple tunnel profiles with consistently scoped routes, gates, logs, and health state
- provider-specific configuration
- tunnel restart / recovery
- streaming diagnostics and log inspection on demand

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
- `tunnelmux-updater` — the Windows delayed-replacement helper

The desktop GUI can also check GitHub Releases from Settings → App Updates. It reads the static `tunnelmux-latest.json` release manifest first and falls back to the GitHub API only when needed. When a newer matching raw `.tar.gz` or `.zip` archive is available, it shows the asset and required SHA-256 before install, downloads into the platform application-config updates directory, rejects unsafe paths / versions / oversized responses, and enables **Restart Now**.

Automatic replacement is limited to raw-binary installations. Native `.dmg`, `.msi`, and `.deb` installations are never overwritten from inside the app bundle; the updater points you to the platform package or Release page instead.

### One-command installer

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Examples:

```bash
# Pin a version
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --version v0.4.0

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

The GUI runs the daemon **inside its own process** — nothing is spawned and there is a single owner of the control port, the state files, and the provider processes. If a local `tunnelmuxd` is already answering on the configured address, the GUI connects to it instead and leaves it running when you quit. Quitting the app stops the daemon it started, along with its tunnels and provider processes; closing the window only hides it to the tray.

The v0.4 console keeps the current tunnel visible in the sidebar and splits work into Overview, Tunnels, Services, Diagnostics, and Settings. Services are rows rather than large cards, so path, upstream, and access mode can be compared side by side; Settings owns language, appearance, control-plane access, and app updates.

If the selected provider is not installed yet, TunnelMux catches that before launch, shows a provider-specific warning, and offers a platform-appropriate install action plus a copyable fallback command instead of surfacing a raw spawn error.

### CLI path

```bash
git clone https://github.com/kexuejin/TunnelMux.git
cd TunnelMux

cargo run -p tunnelmuxd -- \
  --listen 127.0.0.1:4765 \
  --gateway-listen 127.0.0.1:18080

cargo run -p tunnelmux-cli -- --tunnel-id primary routes add \
  --id app-web \
  --upstream-url http://127.0.0.1:3000 \
  --path-prefix /app

cargo run -p tunnelmux-cli -- --tunnel-id primary tunnel start \
  --provider cloudflared \
  --target-url http://127.0.0.1:18080 \
  --auto-restart
```

`--tunnel-id` applies consistently to tunnel status, routes, logs, health, metrics, and dashboard commands. It defaults to `primary` for compatibility, so existing single-tunnel scripts keep working.

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

More release and bundle details live in [docs/RELEASING.md](docs/RELEASING.md).

## FAQ

**How do I expose a local service with a public URL?**
Create a tunnel (`cloudflared` or `ngrok`), start it, then add a service pointing at your local URL (for example `http://127.0.0.1:3000`). TunnelMux shows the public URL and the routing state on each service row.

**How is TunnelMux different from using the `cloudflared` or `ngrok` CLI directly?**
The CLI is great for one tunnel at a time. TunnelMux is a desktop control plane: multi-service host/path routing, health checks, per-route access gates, provider logs, diagnostics, and an in-app updater — in one GUI, with the same daemon/API powering the CLI.

**Can I expose DeepSeek Harness, Ollama, or Open WebUI?**
Yes. Add a service, then use the **DeepSeek / SPA Preset**. It mounts loopback-protected apps under a path (for example `/deepseek` → `http://127.0.0.1:3080`), keeps the original Host header off so the app sees a loopback Host, rewrites response paths, and leaves root `/` closed by default.

**How do I protect my public tunnel routes with an access code?**
Set a default service access code under Settings → Default service access, or choose per-service inherit / custom / public. Visitors unlock each route with the code once; TunnelMux stores a route-scoped cookie so other routes stay unaffected.

**Where are GUI control tokens and provider tokens stored?**
In the operating-system credential store under the `com.tunnelmux.gui` service: macOS Keychain, Windows Credential Manager, or the Linux Secret Service integration. `settings.json` keeps only non-secret configuration. On Linux, a working Secret Service / keyring session is required for GUI token storage.

**Does the in-app updater replace `.dmg`, `.msi`, or `.deb` installations?**
No. Automatic replacement is limited to verified raw `.tar.gz` / `.zip` binary installations. Native bundles direct you to the platform package flow or the GitHub Release page so the app never rewrites files inside a signed application bundle.

**How do I operate a non-default tunnel from the CLI?**
Pass the global `--tunnel-id <id>` before the subcommand. Routes, access gates, logs, health, metrics, status, and dashboard calls then all target that tunnel; omitting it keeps the legacy `primary` default.

**Which platforms does TunnelMux support?**
macOS (Intel + Apple Silicon), Windows, and Linux. GitHub Releases ships raw archives and native installers (`.dmg`, `.msi`, `.deb`) for all three.

**How do I switch the TunnelMux UI to Chinese?**
Use the language selector in the header or Settings → Interface and choose **简体中文**. Auto follows your system language and the choice is remembered between launches.

**How do I keep root `/` closed while exposing a subpath?**
Do not add a service with path `/`. Each service row shows whether root `/` is exposed or stays closed, and the DeepSeek / SPA preset keeps root closed by default.

## Config and credentials

Daemon files:

- `~/.tunnelmux/config.json` — declarative routes and health-check settings
- `~/.tunnelmux/state.json` — daemon-owned runtime snapshot
- `~/.tunnelmux/api-token` — auto-generated control-plane bearer token (owner-only on Unix)
- `~/.tunnelmux/provider.log` — provider stdout/stderr, rotated at 16 MiB into `provider.log.1…3`

The daemon polls `config.json` and applies route and health-check changes without restarting. State, settings, and token writes use unique temporary files plus atomic rename, so a failed write is reported instead of being reported as a successful save.

The GUI keeps its non-secret preferences in `settings.json` inside the OS application-config directory — for example `~/Library/Application Support/com.tunnelmux.gui/settings.json` on macOS. Control bearer tokens, Cloudflare tunnel tokens, and ngrok authtokens are stored separately in the OS credential store; legacy plaintext fields are migrated and removed on load.

Both the daemon token and the provider log live next to the state file, so `--data-file /tmp/scratch/state.json` keeps its token at `/tmp/scratch/api-token` and never touches the token a production daemon handed out. `--api-token-file`, `--provider-log-file`, `--provider-log-max-bytes` (0 disables rotation), and `--provider-log-max-files` (0 keeps no backups) override each path and size individually.

## Service access gates

Public tunnel routes can be protected before traffic reaches the upstream service. Configure a default service access code in Settings → Default service access, then choose a per-service mode in the service drawer:

- `Inherit default gate` — use the default code when one is configured
- `Use custom service code` — require a service-specific code
- `Always public` — opt the service out of the default gate

The daemon stores the default gate in `default_route_access` and per-service overrides in `route_access` inside `~/.tunnelmux/state.json`. Since v0.4.0, each override is keyed by **tunnel + route**, so the same route id in two tunnels keeps independent gates; legacy route-only state is migrated on load. Successful browser unlocks use route-scoped cookies such as `tunnelmux_access_<route_id>`, so protecting one service does not open unrelated routes.

For mounted SPAs such as DeepSeek Harness, use the **DeepSeek / SPA Preset** in the service editor. It sets a path mount, keeps Host forwarding off, enables response path rewriting, and reminds you that root `/` stays closed unless another service explicitly exposes it. Each service row also has **Test** to check the public route and upstream status.

## Security

The control-plane API (`127.0.0.1:4765`) authenticates with a bearer token. `--control-auth` selects the mode: `require` (default), `optional`, or `off`. In `require` mode all protected endpoints demand a valid token; when none is configured the daemon generates one into `api-token` next to the state file — `~/.tunnelmux/api-token` by default.

The CLI, GUI, shared control client, and compatible remote integrations auto-read that token **only when the control base URL resolves to loopback**. Remote daemons must receive a token explicitly through `--token` or `TUNNELMUX_API_TOKEN`; a local secret is never attached to a remote request. `GET /v1/health` is intentionally unauthenticated and does not send the control token.

You can also **unlock loopback** with a human-enterable access code (`--unlock-code <CODE>` or auto-rotated when unset; default window 4h, `--unlock-window <ms>`). While unlocked, local requests pass without a token; non-loopback (for example bridged) access still requires the bearer token. The auth status / unlock / relock endpoints themselves require the control bearer token — the access code unlocks the session, it is not a replacement for endpoint authentication. Use `tunnelmux unlock <code>` / `tunnelmux unlock --show-code` / `--relock`, or the GUI under Settings → Control-plane access.

Additional v0.4.0 boundaries:

- The gateway strips route-gate `Authorization` and `tunnelmux_access_*` cookie material before forwarding to an upstream, while preserving unrelated headers.
- Provider executables come from daemon startup configuration; the HTTP API no longer accepts a per-request provider binary override.
- The updater requires a valid semver, a single safe asset basename, a successful response within the size/time limits, and a matching SHA-256 before touching any binary.
- GUI control and provider tokens live in the OS credential store rather than plaintext settings.

## Docs

- [Documentation site](https://kexuejin.github.io/TunnelMux/)
- [Architecture](docs/ARCHITECTURE.md)
- [API](docs/API.md)
- [Third-Party Integration](docs/INTEGRATION.md)
- [Integration Templates](docs/INTEGRATION-TEMPLATES.md)
- [Roadmap](docs/ROADMAP.md)
- [Releasing](docs/RELEASING.md)
- [v0.4.0 release notes](docs/releases/v0.4.0.md)
- [Changelog](CHANGELOG.md)
- [Promo & growth copy](docs/PROMO.md)
- [简体中文文档](docs/zh/index.md)

## Repository layout

- `crates/tunnelmux-core` — shared domain models and protocol types
- `crates/tunnelmux-control-client` — shared HTTP control client for CLI and GUI
- `crates/tunnelmuxd` — daemon runtime and control-plane API, built as a library plus a thin standalone binary
- `crates/tunnelmux-cli` — CLI client and operational commands
- `crates/tunnelmux-gui` — Tauri desktop console, credential store, updater integration, and Windows delayed-replacement helper
- `scripts/install.sh` — installer for macOS/Linux

## Contributing

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
