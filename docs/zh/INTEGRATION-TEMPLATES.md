---
title: 集成模板
description: 外部系统与 TunnelMux 集成的可直接改写模板（Bash / Node.js / Python）。
---

# 集成模板

本文档为需要与 TunnelMux 集成的外部系统提供可直接改写的模板。

## 目标场景

- 需要临时公网暴露的 CI/CD 自动化
- 管理多个应用路由的本地平台工具
- 自定义控制面板（Web、桌面、内部运维 UI）

## 基础集成契约

外部系统应把 TunnelMux 当作一个 API 依赖，按以下流程使用：

1. 检查隧道状态（`GET /v1/tunnel/status`）
2. 需要时启动隧道（`POST /v1/tunnel/start`）
3. 应用或 upsert 路由（`POST /v1/routes/apply` 或 `PUT /v1/routes/{id}`）
4. 观察运行时健康（`GET /v1/dashboard`、`/v1/upstreams/health`）

## 模板 1：Bash（cURL）

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

# 1) 确保隧道在运行
state="$(curl -fsSL "${auth_header[@]}" "${BASE_URL}/v1/tunnel/status" | jq -r '.tunnel.state // "stopped"')"
if [[ "${state}" != "running" && "${state}" != "starting" ]]; then
  curl -fsSL "${auth_header[@]}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE_URL}/v1/tunnel/start" \
    -d "{\"provider\":\"cloudflared\",\"target_url\":\"${GATEWAY_TARGET}\",\"auto_restart\":true}" >/dev/null
fi

# 2) 幂等应用路由集合
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
        "health_check_enabled": true,
        "enabled": true
      }
    ]
  }' >/dev/null
```

注意事项：

- 需要 `jq`
- 声明式所有权使用 `mode: replace`
- 用 `allow_empty: false` 避免误清空路由

## 模板 2：Node.js（fetch）

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
      health_check_enabled: true,
      enabled: true,
      upsert: true,
    }),
  });
}

await ensureTunnelRunning();
await upsertRoute();
```

## 模板 3：Python（requests）

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
        "health_check_enabled": True,
        "enabled": True,
        "upsert": True,
    },
    timeout=5,
).raise_for_status()
```

## 运维建议

- 让 TunnelMux daemon 与集成主机保持同一台机器
- 开发模式之外始终使用 `TUNNELMUX_API_TOKEN`
- 自动化任务用 `routes/apply` 做声明式所有权管理
- 轮询 `dashboard` 或订阅 SSE 端点获取实时状态
- 为不中断重启定义备用上游
