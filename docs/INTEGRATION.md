# Third-Party Integration Guide

This document describes how any external platform can integrate with TunnelMux.

## Integration Boundary

- TunnelMux owns tunnel lifecycle and gateway routing.
- External platforms act as API clients.
- Business-specific logic stays in external platforms, not inside TunnelMux.

## Recommended Integration Pattern

## 1. Configuration in external platform

Store:
- `TUNNELMUX_BASE_URL` (default: `http://127.0.0.1:4765`)
- `TUNNELMUX_API_TOKEN` (optional but recommended)

## 2. Lifecycle flow

Typical startup flow:
1. `GET /v1/tunnel/status`
2. if not running, call `POST /v1/tunnel/start`
3. ensure route exists (`POST /v1/routes` or `POST /v1/routes/apply`)

## 3. Routing strategy

Recommended order:
- prefer host-based routes for stable multi-service mapping
- use path-based routes when host allocation is limited
- configure fallback upstreams for graceful failover

## 4. Operations model

External platforms can:
- poll `GET /v1/dashboard` for consolidated runtime snapshots
- subscribe to SSE endpoints for live state updates
- read provider logs via `GET /v1/tunnel/logs` or `/stream`

## 5. Security baseline

- bind control API to loopback when possible
- enable API token for non-development environments
- limit token distribution to trusted service components

## Migration Blueprint (for existing in-app tunnel logic)

1. run TunnelMux side-by-side with current tunnel implementation
2. move route lifecycle operations to TunnelMux API
3. keep old logic as fallback during validation
4. remove in-app provider process ownership after cutover

