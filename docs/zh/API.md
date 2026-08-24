---
title: TunnelMux 控制 API
description: TunnelMux 控制面 HTTP API：认证、隧道生命周期、路由管理、设置与访问门禁。
---

# TunnelMux 控制 API

控制面基础 URL：`http://127.0.0.1:4765`

网关 URL：`http://127.0.0.1:18080`（默认；可通过 daemon 参数配置）

运行时 / 配置文件分工：

- `~/.tunnelmux/config.json` 存放声明式路由与健康检查设置
- `~/.tunnelmux/state.json` 存放 daemon 维护的运行时快照
- 当 `config.json` 存在时，daemon 会轮询它并热重载路由 / 健康检查变更

相关 daemon 参数：

- `--data-file <PATH>`（默认 `~/.tunnelmux/state.json`）
- `--config-file <PATH>`（默认 `~/.tunnelmux/config.json`）
- `--config-reload-interval-ms <MS>`（默认 `1000`）
- `--provider-log-file <PATH>`（默认 `~/.tunnelmux/provider.log`）
- `--api-token <TOKEN>`

## 认证

控制面 API 支持 bearer-token 认证层。daemon 通过 `--control-auth <mode>`（默认 `require`）或 `TUNNELMUX_CONTROL_AUTH` 选择模式。

| 模式 | 行为 |
|---|---|
| `require`（默认） | 默认拒绝：每个受保护端点都要求有效的 `Authorization: Bearer <token>`。未配置 token 时，daemon 自动生成一个并写入 `~/.tunnelmux/api-token`（0600），供本地工具自动发现。 |
| `optional` | 向后兼容：配置了 token 就强制执行；没有 token 的 daemon 保持开放。 |
| `off` | 从不强制（仅限本地开发）。 |

可以用 `--api-token <TOKEN>` 或 `TUNNELMUX_API_TOKEN` 显式设置 token。CLI、GUI 与 `dsh-tunnelmux-remote` 使用同一个 token；未传 token 时，这些客户端会回退读取 `~/.tunnelmux/api-token`（若存在）。

`GET /v1/health` 始终免认证。

### 访问码解锁（loopback）

在本机交互使用场景，可以用人工输入的访问码解锁 loopback 控制，而不是在每次请求都传 token。访问码通过 `--unlock-code <CODE>`（固定）设置，未设置时自动生成并在每次重新锁定时轮换。解锁持续 `--unlock-window` 毫秒（默认 4 小时），到期回到锁定。

- **loopback** 请求在解锁窗口开启 *或* 携带有效 bearer token 时允许通过。
- **非 loopback** 请求始终要求有效 bearer token（访问码永远不能解锁外部访问）。

认证端点仅限 loopback：

| 方法 | 路径 | 用途 |
|---|---|---|
| `POST` | `/v1/auth/unlock` `{ "code": "<code>" }` | 在窗口期内解锁 loopback。 |
| `GET` | `/v1/auth/status` | 当前解锁状态 + 当前访问码。 |
| `POST` | `/v1/auth/relock` `{}` | 立即锁定 loopback（未固定时轮换访问码）。 |

CLI：`tunnelmux unlock <code>`、`tunnelmux unlock --show-code`、`tunnelmux unlock --relock`。GUI 在 Settings → Control-plane access 提供同样控制。

## 1. 健康检查

`GET /v1/health`

响应示例：

```json
{
  "ok": true,
  "service": "tunnelmuxd",
  "version": "0.2.0"
}
```

## 2. 隧道生命周期

- `GET /v1/tunnel/status`
- `GET /v1/tunnel/status/stream`
- `GET /v1/tunnel/logs`
- `GET /v1/tunnel/logs/stream`
- `POST /v1/tunnel/start`
- `POST /v1/tunnel/stop`

与生命周期行为相关的 daemon 参数：

- `--max-auto-restarts <N>`（默认 `10`）
- `--health-check-interval-ms <MS>`（默认 `5000`）
- `--health-check-timeout-ms <MS>`（默认 `2000`）
- `--health-check-path <PATH>`（默认 `/`）

`POST /v1/tunnel/start` 示例：

```json
{
  "provider": "cloudflared",
  "target_url": "http://127.0.0.1:18080",
  "auto_restart": true
}
```

`GET /v1/tunnel/status` 示例：

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

日志尾部示例：

```bash
curl -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs?lines=100"
```

日志流示例（SSE）：

```bash
curl -N -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs/stream?lines=50&poll_ms=1000"
```

## 3. 路由管理

- `GET /v1/routes`
- `GET /v1/routes/stream`
- `GET /v1/routes/match`
- `POST /v1/routes`
- `POST /v1/routes/apply`
- `PUT /v1/routes/{id}`
- `DELETE /v1/routes/{id}`

`POST /v1/routes` 示例：

```json
{
  "id": "app-web",
  "match_host": "app.example.com",
  "match_path_prefix": "/",
  "strip_path_prefix": null,
  "upstream_url": "http://127.0.0.1:3000",
  "fallback_upstream_url": "http://127.0.0.1:3001",
  "health_check_path": "/healthz",
  "health_check_enabled": true,
  "enabled": true
}
```

`health_check_enabled` 可选，默认 `true`。设为 `false` 可保持路由启用，同时将其上游排除在活跃健康探测与基于健康状态的 failover 排序之外。

`GET /v1/routes` 示例：

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

## 4. 设置

- `GET /v1/settings/health-check`
- `PUT /v1/settings/health-check`
- `POST /v1/settings/reload`

`GET /v1/settings/health-check` 示例：

```json
{
  "health_check": {
    "interval_ms": 5000,
    "timeout_ms": 2000,
    "path": "/"
  }
}
```

`PUT /v1/settings/health-check` 请求示例：

```json
{
  "interval_ms": 7500,
  "timeout_ms": 1500,
  "path": "/readyz"
}
```

`PUT /v1/settings/health-check` 响应示例：

```json
{
  "health_check": {
    "interval_ms": 7500,
    "timeout_ms": 1500,
    "path": "/readyz"
  }
}
```

`POST /v1/settings/reload` 立即触发一次设置刷新。

## 5. 路由访问门禁

公网路由可以在流量进入上游之前先要求访问码。相关端点：

- `GET /v1/routes/access` — 汇总默认门禁与每个路由的门禁状态
- `POST /v1/routes/access` — 设置默认门禁或某个路由的门禁

`POST /v1/routes/access` 请求示例：

```json
{
  "route_id": "__default__",
  "require_access_code": "secret-code",
  "public": null,
  "cookie_ttl_ms": 3600000
}
```

- `route_id` 为 `__default__`（或 `default` / `*`）时设置全局默认门禁
- 对普通路由设置 `require_access_code` 使用自定义访问码
- `public: true` 让该路由显式公开

## 6. 诊断与观测

- `GET /v1/dashboard` — 汇总运行时快照（隧道、路由、健康、配置）
- `GET /v1/diagnostics` — 数据文件 / 配置文件 / provider 日志 / 路由计数等诊断信息
- `GET /v1/upstreams/health` — 上游健康快照
- `GET /v1/metrics` — 运行时指标快照

更多细节见英文原版 [API 文档](../API.md)。
