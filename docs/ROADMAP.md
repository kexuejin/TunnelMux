# TunnelMux Roadmap

## Phase 0: Foundation (Completed)

- [x] Rust workspace bootstrapped
- [x] daemon + CLI baseline commands
- [x] initial architecture and API documentation

## Phase 1: Minimum Viable Product (Completed)

- [x] provider process lifecycle (`cloudflared` / `ngrok`) baseline
- [x] tunnel status persistence
- [x] route configuration persistence
- [x] CLI operations: `tunnel` and `routes`
- [x] provider auto-restart strategy
- [x] provider log persistence and streaming
- [x] token-protected control-plane API

## Phase 2: Gateway and Routing (Completed)

- [x] host/path route matching
- [x] HTTP reverse proxy
- [x] WebSocket proxying
- [x] route-level primary/fallback failover
- [x] active health checks
- [x] `wss/https` upstream support baseline

## Phase 3: Productization and Ecosystem Integration (In Progress)

- [x] generic third-party integration templates
- [ ] GUI MVP (Tauri)
- [x] hot configuration reload
- [x] operational audit and diagnostics

## Phase 4: Advanced Capabilities

- [ ] hardened multi-tenant isolation model
- [ ] signed short-link management API
- [ ] provider plugin model and extension APIs
- [ ] observability (metrics, tracing, profiling)

