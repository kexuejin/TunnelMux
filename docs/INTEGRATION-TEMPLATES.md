# Integration Templates

This document provides ready-to-adapt templates for external systems integrating with TunnelMux.

## Target Use Cases

- CI/CD automation that needs temporary public exposure
- local platform tools that manage multiple app routes
- custom control panels (web, desktop, internal ops UI)

## Baseline Integration Contract

External systems should treat TunnelMux as an API dependency with this flow:

1. Check tunnel state (`GET /v1/tunnel/status`)
2. Start tunnel when needed (`POST /v1/tunnel/start`)
3. Apply or upsert routes (`POST /v1/routes/apply` or `PUT /v1/routes/{id}`)
4. Observe runtime health (`GET /v1/dashboard`, `/v1/upstreams/health`)

## Template 1: Bash (cURL)

```bash
#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${TUNNELMUX_BASE_URL:-http://127.0.0.1:4765}"
TOKEN="${TUNNELMUX_API_TOKEN:-}"
GATEWAY_TARGET="${TUNNELMUX_GATEWAY_TARGET:-http://127.0.0.1:18080}"

auth_header=()
if [[ -n "${TOKEN}" ]]; then
  auth_header=(-H "Authorization: Bearer ${TOKEN}")
fi

# 1) Ensure tunnel is running
state="$(curl -fsSL "${auth_header[@]}" "${BASE_URL}/v1/tunnel/status" | jq -r '.tunnel.state // "stopped"')"
if [[ "${state}" != "running" && "${state}" != "starting" ]]; then
  curl -fsSL "${auth_header[@]}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE_URL}/v1/tunnel/start" \
    -d "{\"provider\":\"cloudflared\",\"target_url\":\"${GATEWAY_TARGET}\",\"auto_restart\":true}" >/dev/null
fi

# 2) Apply route set idempotently
curl -fsSL "${auth_header[@]}" \
  -H "Content-Type: application/json" \
  -X POST "${BASE_URL}/v1/routes/apply" \
  -d '{
    "mode": "replace",
    "allow_empty": false,
    "routes": [
      {
        "id": "app-web",
        "match_host": "app.local",
        "match_path_prefix": "/",
        "upstream_url": "http://127.0.0.1:3000",
        "fallback_upstream_url": "http://127.0.0.1:3001",
        "health_check_path": "/healthz",
        "enabled": true
      }
    ]
  }' >/dev/null
```

Notes:
- requires `jq`
- use `mode: replace` for declarative ownership
- use `allow_empty: false` to avoid accidental route wipe

## Template 2: Node.js (fetch)

```js
const baseUrl = process.env.TUNNELMUX_BASE_URL ?? "http://127.0.0.1:4765";
const token = process.env.TUNNELMUX_API_TOKEN ?? "";

const headers = token ? { Authorization: `Bearer ${token}` } : {};

async function ensureTunnelRunning() {
  const statusRes = await fetch(`${baseUrl}/v1/tunnel/status`, { headers });
  const status = await statusRes.json();
  const state = status?.tunnel?.state ?? "stopped";

  if (state === "running" || state === "starting") return;

  await fetch(`${baseUrl}/v1/tunnel/start`, {
    method: "POST",
    headers: { ...headers, "Content-Type": "application/json" },
    body: JSON.stringify({
      provider: "cloudflared",
      target_url: "http://127.0.0.1:18080",
      auto_restart: true,
    }),
  });
}

async function upsertRoute() {
  await fetch(`${baseUrl}/v1/routes/app-web`, {
    method: "PUT",
    headers: { ...headers, "Content-Type": "application/json" },
    body: JSON.stringify({
      id: "app-web",
      match_host: "app.local",
      match_path_prefix: "/",
      upstream_url: "http://127.0.0.1:3000",
      fallback_upstream_url: "http://127.0.0.1:3001",
      health_check_path: "/healthz",
      enabled: true,
      upsert: true,
    }),
  });
}

await ensureTunnelRunning();
await upsertRoute();
```

## Template 3: Python (requests)

```python
import os
import requests

base_url = os.getenv("TUNNELMUX_BASE_URL", "http://127.0.0.1:4765")
token = os.getenv("TUNNELMUX_API_TOKEN", "")

headers = {"Authorization": f"Bearer {token}"} if token else {}

status = requests.get(f"{base_url}/v1/tunnel/status", headers=headers, timeout=5).json()
state = (status.get("tunnel") or {}).get("state", "stopped")

if state not in ("running", "starting"):
    requests.post(
        f"{base_url}/v1/tunnel/start",
        headers={**headers, "Content-Type": "application/json"},
        json={
            "provider": "cloudflared",
            "target_url": "http://127.0.0.1:18080",
            "auto_restart": True,
        },
        timeout=5,
    ).raise_for_status()

requests.put(
    f"{base_url}/v1/routes/app-web",
    headers={**headers, "Content-Type": "application/json"},
    json={
        "id": "app-web",
        "match_host": "app.local",
        "match_path_prefix": "/",
        "upstream_url": "http://127.0.0.1:3000",
        "fallback_upstream_url": "http://127.0.0.1:3001",
        "health_check_path": "/healthz",
        "enabled": True,
        "upsert": True,
    },
    timeout=5,
).raise_for_status()
```

## Operational Recommendations

- keep TunnelMux daemon local to the integration host
- always use `TUNNELMUX_API_TOKEN` outside development mode
- use `routes/apply` for declarative ownership from automation jobs
- poll `dashboard` or subscribe to SSE endpoints for live status
- define fallback upstreams for non-disruptive restarts
