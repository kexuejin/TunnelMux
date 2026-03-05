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

## 3. `tunnelmux-cli`

Responsibilities:
- default operational control surface
- invoke daemon API for all lifecycle and route operations
- provide both human-friendly and machine-readable output modes

## 4. GUI layer (optional)

GUI (for example, Tauri) is an API client of `tunnelmuxd`.
It does not manage provider processes directly.

## Design Principles

- single tunnel, multiple local service routes
- clear control-plane/data-plane separation
- API-first integration surface
- local-first security (loopback binding + optional bearer token)
- caller-independent design (no embedded business adapters)

