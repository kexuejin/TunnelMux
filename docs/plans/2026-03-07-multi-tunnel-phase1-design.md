# Multi-Tunnel Phase 1 Design

**Date:** 2026-03-07

**Scope**
- Move the product model from “global tunnel settings” to “current tunnel context”.
- Introduce `Tunnel Profile` as a first-class concept in the GUI and daemon API design.
- Keep the default experience simple: zero or one visible tunnel, with multi-tunnel controls only appearing when the user explicitly creates more.

## Problem

The current GUI still assumes a single global tunnel configuration. That works for the simplest case, but it breaks down once a user wants:

- different providers for different use cases;
- separate service groups behind separate public endpoints;
- independent tunnel-specific configuration and lifecycle.

At the same time, the GUI must remain easy to use for the default user who only needs one tunnel.

## Product Model

### Default behavior
- A new user starts with **zero tunnels**.
- The empty state shows a single primary action: `Create Tunnel`.
- No tunnel is auto-created because the system cannot safely guess the intended provider.

### Tunnel-first model
- The GUI always manages the **currently selected tunnel**.
- Services belong to exactly one tunnel.
- Tunnel configuration is handled in `Create Tunnel` and `Edit Tunnel`, not in a global settings center.

### Visibility rules
- `0 tunnels`: show empty state only.
- `1 tunnel`: do not show a tunnel switcher.
- `2+ tunnels`: show a tunnel switcher, recommended as a dropdown rather than tabs.

## Tunnel Creation Flow

### Create Tunnel dialog
- Default-filled, but never auto-submitted.
- Recommended default: `cloudflared / Quick Tunnel`.
- User can:
  - `Start Now`
  - expand `Advanced`
  - switch provider before creating

### Provider-specific configuration
- `cloudflared`
  - Quick Tunnel
  - Named Tunnel (token)
- `ngrok`
  - authtoken
  - reserved domain

### Edit Tunnel
- Applies only to the current tunnel.
- Tunnel configuration remains separate from services.
- If runtime-affecting settings change, the user can choose `Save` or `Save and Restart`.

## Settings Model

### Keep
- Application-level settings only:
  - daemon/control endpoint
  - control token
  - import/export
  - debugging / diagnostics toggles

### Remove from global settings
- provider selection
- provider auth/domain/token
- tunnel-local gateway port
- tunnel name and lifecycle defaults

These move into tunnel creation/editing.

## Runtime Architecture Direction

### Target architecture
- One `tunnelmuxd`
- Multiple tunnel runtimes/workers
- Each tunnel owns:
  - provider process
  - provider config
  - gateway port
  - services
  - status / restart policy / logs

### Recommended delivery strategy

#### Phase 1
- Introduce `Tunnel Profile` in data model and GUI flow.
- Rebuild the GUI around current-tunnel context.
- Keep the UX consistent with future multi-tunnel runtime support.

#### Phase 2
- Add true multi-tunnel concurrent runtime support in the daemon.
- Each tunnel gets an independent gateway port and lifecycle.

#### Phase 3
- Add tunnel-scoped diagnostics, persistence polish, import/export, and recovery quality.

## Why this is Phase 1 only

The data model and GUI can move to tunnel-first before the daemon fully supports concurrent tunnel workers. This keeps the user-facing mental model stable while lowering implementation risk.

## Non-Goals for Phase 1

- Full multi-tunnel concurrent runtime.
- Multiple provider processes running in parallel.
- Tunnel-scoped diagnostics UI.
- Import/export of tunnel profiles.
