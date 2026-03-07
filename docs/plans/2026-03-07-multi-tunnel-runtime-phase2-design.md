# Multi-Tunnel Runtime Phase 2 Design

**Date:** 2026-03-07

**Scope**
- Introduce true concurrent multi-tunnel runtime support in `tunnelmuxd`.
- Allow multiple tunnel profiles to run at the same time.
- Keep the GUI focused on one selected tunnel while other tunnels continue running independently.

## Problem

Phase 1 moved the GUI and settings to a tunnel-first model, but the daemon still has a single runtime:

- one `running_tunnel`
- one `pending_restart`
- one global route set
- one gateway port

That means multiple tunnel profiles can exist in the UI, but they cannot yet behave as independently running tunnels.

## Core Decision

Use **one daemon, many tunnel workers**.

Do **not** spawn one daemon per tunnel.

### Why
- one control API is simpler for the GUI
- status, logs, metrics, and import/export stay centralized
- runtime coordination is easier than cross-daemon orchestration

## Runtime Model

### New runtime shape
- `RuntimeState`
  - `tunnels: HashMap<TunnelId, TunnelRuntimeState>`
  - `current_tunnel_id: Option<TunnelId>` for GUI selection only
  - shared health-check / diagnostics settings remain global unless explicitly made tunnel-scoped later

### `TunnelRuntimeState`
- persisted tunnel profile
- persisted routes for that tunnel
- `running_tunnel: Option<RunningTunnel>`
- `pending_restart: Option<PendingRestart>`
- provider log file path
- tunnel-local gateway listen address/port

## Tunnel Semantics

### Selected vs Running
- **Selected tunnel**
  - UI context only
  - which tunnel the GUI is currently showing
- **Running tunnel**
  - runtime state per tunnel
  - any number of tunnels may be running simultaneously

These must not be conflated.

### Start / Stop
- act on a specific tunnel id
- never affect other tunnels

### Restart / Auto Restart
- per tunnel
- restart budget and backoff tracked independently

## Routing Model

### Route ownership
- every route has a `tunnel_id`
- route APIs become tunnel-scoped

### Gateway isolation
- each tunnel has its own gateway port
- each provider process targets that tunnel’s gateway port
- gateway only sees routes belonging to that tunnel

This avoids cross-tunnel route leakage.

## API Model

### New shape
- tunnel workspace endpoints return all tunnel summaries
- tunnel lifecycle endpoints require `tunnel_id`
- route CRUD requires `tunnel_id`
- diagnostics/log endpoints become tunnel-scoped or accept `tunnel_id`

### Backward compatibility
- Not required for this phase according to current product direction.
- Direct schema/API change is acceptable.

## GUI Model

- top dropdown selects current tunnel
- current page shows only:
  - current tunnel status
  - current tunnel services
  - current tunnel provider status
  - current tunnel diagnostics
- background tunnels remain running even when not selected

## Delivery Strategy

### Phase 2A
- introduce tunnel-scoped runtime structs
- start/stop a specific tunnel
- independent gateway ports
- tunnel-scoped routes

### Phase 2B
- tunnel-scoped diagnostics and logs
- tunnel-scoped metrics/status streams
- provider-status card reads from current tunnel only

### Phase 2C
- cleanup UX
- tunnel import/export
- recovery polish after daemon restart

## Non-Goals

- Cloudflare API automation
- multi-daemon orchestration
- cluster/distributed runtime
