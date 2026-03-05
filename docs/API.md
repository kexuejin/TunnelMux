# TunnelMux 控制 API（当前实现 + 草案）

Base URL: `http://127.0.0.1:4765`

Gateway URL: `http://127.0.0.1:18080`（默认，可通过 daemon 参数调整）

鉴权：
- 当 daemon 配置了 `--api-token`（或环境变量 `TUNNELMUX_API_TOKEN`）时，除 `GET /v1/health` 外的控制面接口都需要：
  - `Authorization: Bearer <token>`
- 未配置 token 时，控制面接口保持无鉴权（便于本机开发）

## 1. 健康检查

`GET /v1/health`

响应示例：
```json
{
  "ok": true,
  "service": "tunnelmuxd",
  "version": "0.1.0"
}
```

## 2. 隧道控制

- `GET /v1/tunnel/status`
- `GET /v1/tunnel/logs`
- `GET /v1/tunnel/logs/stream`
- `POST /v1/tunnel/start`
- `POST /v1/tunnel/stop`

daemon 关键参数（非 API）：
- `--max-auto-restarts <N>`：provider 异常退出后最大自动重启次数（默认 `10`）
- `--provider-log-file <PATH>`：provider stdout/stderr 持久化日志路径（默认 `~/.tunnelmux/provider.log`）
- `--api-token <TOKEN>`：启用控制面 Bearer Token 鉴权
- `--health-check-interval-ms <MS>`：上游主动健康探测间隔（默认 `5000`）
- `--health-check-timeout-ms <MS>`：单次健康探测超时（默认 `2000`）
- `--health-check-path <PATH>`：主动探测请求路径（默认 `/`，例如 `/healthz`）

`tunnel/start` 请求示例：
```json
{
  "provider": "cloudflared",
  "target_url": "http://127.0.0.1:18080",
  "auto_restart": true
}
```

说明：
- provider 异常退出时，如果 `auto_restart=true`，状态会进入 `starting` 并执行指数退避重启（1/2/4/8/16/32 秒，32 秒封顶）
- `restart_count` 会记录当前已执行/计划的自动重启次数
- 超过 `--max-auto-restarts` 后，状态会进入 `error`

`tunnel/status` 响应示例：
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

`tunnel/logs` 请求示例：
```bash
curl -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs?lines=100"
```

`tunnel/logs` 响应示例：
```json
{
  "lines": [
    "2026-03-05T12:00:00+00:00 [cloudflared:stdout] ...",
    "2026-03-05T12:00:01+00:00 [cloudflared:stderr] ..."
  ]
}
```

说明：
- `lines` 为可选参数，默认 `200`，范围 `1..=5000`
- 当日志文件不存在时，返回空数组

`tunnel/logs/stream` 请求示例（SSE）：
```bash
curl -N -H "Authorization: Bearer dev-token" \
  "http://127.0.0.1:4765/v1/tunnel/logs/stream?lines=50&poll_ms=1000"
```

说明：
- 返回 `text/event-stream`
- 每条日志通过 `event: line` + `data: ...` 推送
- 参数：
  - `lines`：初始回放日志行数，默认 `200`，范围 `1..=5000`
  - `poll_ms`：轮询间隔（毫秒），默认 `1000`，范围 `100..=10000`

## 3. 路由管理

- `GET /v1/routes`
- `POST /v1/routes`
- `DELETE /v1/routes/:id`

`POST /v1/routes` 请求示例：
```json
{
  "id": "solomesh-web",
  "match_host": "solomesh.example.com",
  "match_path_prefix": "/",
  "strip_path_prefix": null,
  "upstream_url": "http://127.0.0.1:3000",
  "fallback_upstream_url": "http://127.0.0.1:3001",
  "health_check_path": "/healthz",
  "enabled": true
}
```

`GET /v1/routes` 响应示例：
```json
{
  "routes": [
    {
      "id": "solomesh-web",
      "match_host": "solomesh.example.com",
      "match_path_prefix": null,
      "strip_path_prefix": null,
      "upstream_url": "http://127.0.0.1:3000",
      "fallback_upstream_url": "http://127.0.0.1:3001",
      "health_check_path": "/healthz",
      "enabled": true
    }
  ]
}
```

## 4. 上游健康状态

- `GET /v1/upstreams/health`

响应示例：
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
    },
    {
      "upstream_url": "http://127.0.0.1:3002",
      "health_check_path": "/",
      "healthy": null,
      "last_checked_at": null,
      "last_error": null
    }
  ]
}
```

说明：
- 该接口返回当前路由表里出现过的 `upstream_url` 与 `fallback_upstream_url` 去重后的健康快照
- 去重维度为 `(upstream_url, health_check_path)`，同一 upstream 在不同探测路径下会分别展示
- `healthy=null` 表示该 upstream 尚未采集到健康结果（例如刚创建路由）
- 默认由 daemon 参数 `--health-check-interval-ms`、`--health-check-timeout-ms`、`--health-check-path` 控制探测频率、超时和探测路径

## 5. 网关转发（当前实现）

- 网关会读取当前路由表并做匹配转发
- 匹配策略：
  - 仅匹配 `enabled=true` 的路由
  - `match_host` 命中优先于无 host 约束
  - `match_path_prefix` 越长优先级越高
- 失败切换策略：
  - 主动探测会周期检查 `upstream_url` / `fallback_upstream_url` 健康状态（HTTP `2xx` 视为健康）
  - 当主上游被探测为不健康且 fallback 健康时，请求顺序会优先尝试 fallback
  - 健康状态未知时保持主上游优先
  - 被动失败切换仍然生效：当当前目标请求失败，或返回 `5xx` 时，会自动重试下一目标
  - WebSocket 握手场景同样适用该策略（握手失败/5xx 时切换）
- 请求示例：

```bash
curl -H 'Host: solomesh.local' http://127.0.0.1:18080/
```

- WebSocket 透传：
  - 当请求包含 `Connection: Upgrade` + `Upgrade: websocket` 时，网关会执行双向透传
  - 已支持 `ws/http` 与 `wss/https` upstream（通过 TLS connector）

## 6. 链接管理（可选）

- `POST /v1/links/issue`
- `POST /v1/links/revoke`
- `GET /v1/links`

> 链接管理端点当前尚未实现，属于后续阶段。
