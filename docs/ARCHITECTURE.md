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
- store only local GUI connection settings (daemon `base_url` and optional token)
- call Tauri commands that delegate to the shared control client
- surface dashboard, tunnel controls, and route CRUD without owning daemon lifecycle

The current GUI MVP intentionally does **not** include:
- daemon auto-launch,
- tray/background integrations,
- real-time log streaming,
- diagnostics/upstream-health pages.

## Design Principles

- single tunnel, multiple local service routes
- clear control-plane/data-plane separation
- API-first integration surface
- local-first security (loopback binding + optional bearer token)
- explicit config/runtime separation (`config.json` desired state vs `state.json` runtime snapshot)
- caller-independent design (no embedded business adapters)
- equal-client model (`CLI` and `GUI` are peers over the same daemon API)
