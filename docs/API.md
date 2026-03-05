# TunnelMux Control API

Base control URL: `http://127.0.0.1:4765`

Gateway URL: `http://127.0.0.1:18080` (default; configurable via daemon flags)

## Authentication

When `--api-token` (or `TUNNELMUX_API_TOKEN`) is configured:
- all control-plane endpoints except `GET /v1/health` require:
  - `Authorization: Bearer <token>`

Without a token, control-plane endpoints are open for local development.

## 1. Health

`GET /v1/health`

Example response:

```json
{
  "ok": true,
  "service": "tunnelmuxd",
  "version": "0.1.3"
}
```

## 2. Tunnel Lifecycle

- `GET /v1/tunnel/status`
- `GET /v1/tunnel/status/stream`
- `GET /v1/tunnel/logs`
- `GET /v1/tunnel/logs/stream`
- `POST /v1/tunnel/start`
- `POST /v1/tunnel/stop`

Daemon flags related to lifecycle behavior:
- `--max-auto-restarts <N>` (default `10`)
- `--provider-log-file <PATH>` (default `~/.tunnelmux/provider.log`)
- `--api-token <TOKEN>`
- `--health-check-interval-ms <MS>` (default `5000`)
- `--health-check-timeout-ms <MS>` (default `2000`)
- `--health-check-path <PATH>` (default `/`)

`POST /v1/tunnel/start` example:

```json
{
  "provider": "cloudflared",
  "target_url": "http://127.0.0.1:18080",
  "auto_restart": true
}
```

`GET /v1/tunnel/status` example:

```json
{
  "tunnel": {
    "state": "running",
    "provider": "cloudflared",
    "target_url": "http://127.0.0.1:18080",
    "public_base_url": "https://xxxx.trycloudflare.com",
    "started_at": "2026-03-05T08:00:00+00:00",
    "updated_at": "2026-03-05T08:00:10+00:00",
    "process_id": 12345,
    "auto_restart": true,
    "restart_count": 0,
    "last_error": null
  }
}
```

Log tail example:

```bash
curl -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs?lines=100"
```

Log stream example (SSE):

```bash
curl -N -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs/stream?lines=50&poll_ms=1000"
```

## 3. Route Management

- `GET /v1/routes`
- `GET /v1/routes/stream`
- `GET /v1/routes/match`
- `POST /v1/routes`
- `POST /v1/routes/apply`
- `PUT /v1/routes/{id}`
- `DELETE /v1/routes/{id}`

`POST /v1/routes` example:

```json
{
  "id": "app-web",
  "match_host": "app.example.com",
  "match_path_prefix": "/",
  "strip_path_prefix": null,
  "upstream_url": "http://127.0.0.1:3000",
  "fallback_upstream_url": "http://127.0.0.1:3001",
  "health_check_path": "/healthz",
  "enabled": true
}
```

`GET /v1/routes` example:

```json
{
  "routes": [
    {
      "id": "app-web",
      "match_host": "app.example.com",
      "match_path_prefix": "/",
      "strip_path_prefix": null,
      "upstream_url": "http://127.0.0.1:3000",
      "fallback_upstream_url": "http://127.0.0.1:3001",
      "health_check_path": "/healthz",
      "enabled": true
    }
  ]
}
```

## 4. Upstream Health

- `GET /v1/upstreams/health`
- `GET /v1/upstreams/health/stream`

Example response:

```json
{
  "upstreams": [
    {
      "upstream_url": "http://127.0.0.1:3000",
      "health_check_path": "/healthz",
      "healthy": true,
      "last_checked_at": "2026-03-05T09:30:00+00:00",
      "last_error": null
    },
    {
      "upstream_url": "http://127.0.0.1:3001",
      "health_check_path": "/healthz",
      "healthy": false,
      "last_checked_at": "2026-03-05T09:30:00+00:00",
      "last_error": "status 503"
    }
  ]
}
```

Notes:
- dedupe key is `(upstream_url, health_check_path)`
- `healthy = null` means no probe result yet

## 5. Gateway Forwarding Behavior

Routing behavior:
- only `enabled = true` routes are considered
- host-specific routes are preferred over host-agnostic routes
- longer `match_path_prefix` has higher priority

Failover behavior:
- active health checks probe primary/fallback upstreams
- if primary is unhealthy and fallback is healthy, fallback is preferred
- passive failover still applies when request execution fails or returns `5xx`
- same behavior applies for WebSocket handshake failures

HTTP forwarding example:

```bash
curl -H 'Host: app.local' http://127.0.0.1:18080/
```

WebSocket forwarding:
- request must include `Connection: Upgrade` and `Upgrade: websocket`
- both `ws/http` and `wss/https` upstreams are supported
