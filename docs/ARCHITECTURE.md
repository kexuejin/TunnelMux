# TunnelMux Architecture

## Positioning

TunnelMux is a standalone infrastructure component that provides:
- tunnel lifecycle control (`start`, `stop`, `status`)
- reverse-proxy routing (`host/path` -> local upstream)
- a local control-plane API for automation and external integration

TunnelMux is intentionally product-agnostic.

## Core Components

## 1. `tunnelmuxd` (Rust daemon)

Responsibilities:
- load declarative config from `config.json` and hot-reload route/health settings
- expose control-plane API (default: `127.0.0.1:4765`)
- manage provider processes (`cloudflared`, `ngrok`)
- store and serve runtime state and route configuration
- supervise provider lifecycle with backoff auto-restart
- expose provider logs and SSE log streams
- expose upstream health snapshots and streams

`tunnelmuxd` builds as **both a library and a binary**. The library exposes
`DaemonArgs`, `start()`, and `serve()`; the binary is a thin CLI wrapper around
them. Headless deployments run the binary. The desktop app links the library and
hosts the same daemon in-process, which is what makes it the single owner of the
control port, the state files, the api token, and the provider child processes.

## 2. Gateway data plane

Responsibilities:
- receive ingress traffic from the active tunnel endpoint
- match and forward requests by `host/path`
- support HTTP + WebSocket upgrade forwarding
- apply primary/fallback failover strategy
- use active health-check signals to prefer healthy targets

## 3. `tunnelmux-control-client`

Responsibilities:
- provide a shared Rust HTTP client for non-streaming control-plane operations
- centralize bearer token handling and structured API error decoding
- keep CLI and GUI request behavior aligned

## 4. `tunnelmux-cli`

Responsibilities:
- default operational control surface
- invoke daemon API for lifecycle, routing, diagnostics, and settings operations
- provide both human-friendly and machine-readable output modes
- keep streaming/log flows as terminal-oriented workflows

## 5. `tunnelmux-gui` (Tauri desktop shell)

Responsibilities:
- present a local operations console for operators
- **host the daemon in-process** by linking the `tunnelmuxd` library, so the app
  is the only owner of the tunnel lifecycle
- persist the connection settings it discovers (daemon `base_url` + token) so a
  cold start reconnects to the same port
- call Tauri commands that delegate to the shared control client
- surface dashboard, tunnel controls, route CRUD, diagnostics, and a tray icon

Ownership rules:
- if a daemon already answers on the configured address, the GUI adopts it and
  never stops it — that is the path for a headless `tunnelmuxd` used by the CLI
- otherwise the GUI starts the embedded daemon and stops it on exit, terminating
  the provider processes it owns

The GUI intentionally does **not**:
- ship or spawn a separate `tunnelmuxd` sidecar binary,
- fall back to a `PATH`-resolved daemon when the configured one is unreachable,
- auto-launch diagnostics subscriptions before a tunnel exists.

## Design Principles

- single tunnel, multiple local service routes
- clear control-plane/data-plane separation
- API-first integration surface
- local-first security (loopback binding + optional bearer token)
- single owner per resource (one control port, one state file, one provider set)
- explicit config/runtime separation (`config.json` desired state vs `state.json` runtime snapshot)
- caller-independent design (no embedded business adapters)
- equal-client model (`CLI` and `GUI` are peers over the same daemon API)
