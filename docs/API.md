# TunnelMux Control API

Base control URL: `http://127.0.0.1:4765`

Gateway URL: `http://127.0.0.1:18080` (default; configurable via daemon flags)

Runtime/config file split:
- `~/.tunnelmux/config.json` stores declarative routes and health-check settings
- `~/.tunnelmux/state.json` stores the daemon-owned runtime snapshot
- when `config.json` exists, the daemon polls it and hot-reloads route / health-check changes

Relevant daemon flags:
- `--data-file <PATH>` (default `~/.tunnelmux/state.json`)
- `--config-file <PATH>` (default `~/.tunnelmux/config.json`)
- `--config-reload-interval-ms <MS>` (default `1000`)
- `--provider-log-file <PATH>` (default `~/.tunnelmux/provider.log`)
- `--api-token <TOKEN>`

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
  "version": "0.1.4"
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

## 4. Settings

- `GET /v1/settings/health-check`
- `PUT /v1/settings/health-check`
- `POST /v1/settings/reload`

`GET /v1/settings/health-check` example:

```json
{
  "health_check": {
    "interval_ms": 5000,
    "timeout_ms": 2000,
    "path": "/"
  }
}
```

`PUT /v1/settings/health-check` request example:

```json
{
  "interval_ms": 7500,
  "timeout_ms": 1500,
  "path": "/readyz"
}
```

`PUT /v1/settings/health-check` response example:

```json
{
  "health_check": {
    "interval_ms": 7500,
    "timeout_ms": 1500,
    "path": "/readyz"
  }
}
```

`POST /v1/settings/reload` forces an immediate settings refresh.

Reload behavior:
- if `config.json` exists, reload prefers the declarative config file
- if `config.json` does not exist, reload falls back to `state.json`
- the current tunnel runtime state is preserved; routes and health-check settings are refreshed
- request body is optional; an empty JSON object is accepted

`POST /v1/settings/reload` response example:

```json
{
  "reloaded": true,
  "route_count": 1,
  "tunnel_state": "error"
}
```

## 5. Diagnostics

- `GET /v1/diagnostics`

Example response:

```json
{
  "data_file": "/Users/example/.tunnelmux/state.json",
  "config_file": "/Users/example/.tunnelmux/config.json",
  "provider_log_file": "/Users/example/.tunnelmux/provider.log",
  "route_count": 2,
  "enabled_route_count": 1,
  "tunnel_state": "running",
  "pending_restart": true,
  "config_reload_enabled": true,
  "config_reload_interval_ms": 1000,
  "last_config_reload_at": null,
  "last_config_reload_error": null
}
```

Notes:
- `config_reload_enabled` indicates whether background polling is active
- `last_config_reload_at` reports the last successful declarative config apply time
- `last_config_reload_error` reports the latest polling / parse error while keeping the last good config active

## 6. Upstream Health

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

## 7. Gateway Forwarding Behavior

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
