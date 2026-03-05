# TunnelMux 架构

## 1. 定位

TunnelMux 是独立于业务系统的基础设施组件：
- 负责隧道生命周期（start/stop/status）
- 负责反向代理路由（host/path -> local upstream）
- 对外提供本地控制 API

## 2. 核心组件

1. `tunnelmuxd` (Rust daemon)
- 本地监听控制 API（默认 `127.0.0.1:4765`）
- 管理 provider 进程（cloudflared/ngrok）
- 管理路由表、状态、日志
- 后台监控 provider 生命周期，异常退出后按退避策略自动重启
- 提供日志查询与日志流（SSE）接口，供 CLI / Tauri 面板复用
- 提供上游健康快照接口（`GET /v1/upstreams/health`）
- 主动健康探测支持可配置探测路径（`--health-check-path`）

2. 转发网关层
- 单入口端口接收来自隧道的流量
- 根据路由规则分发到不同本机端口
- 支持 HTTP + WebSocket Upgrade（含 `wss/https` 上游）
- 支持 route 级 fallback upstream 失败切换（主上游失败/5xx 自动切换）
- 支持主动健康探测并影响转发优先级（主不健康且 fallback 健康时优先 fallback）
- 健康探测支持路由级路径覆盖（`RouteRule.health_check_path` 优先于 daemon 默认路径）

3. `tunnelmux-cli`
- 默认控制面
- 调用 daemon API 做运维操作

4. Tauri GUI
- 可选图形控制台
- 复用 daemon API，不直连 provider

## 3. 设计原则

- 单隧道多路由
- 控制面和数据面分离
- 业务系统（如 SoloMesh）仅做客户端集成
- 默认本地监听 + token 鉴权
