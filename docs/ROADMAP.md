# TunnelMux Roadmap

## Phase 0: 骨架（已完成）

- [x] Rust workspace 建立
- [x] daemon + cli 基础命令
- [x] 文档和 API 草案

## Phase 1: 最小可用（进行中）

- [x] cloudflared / ngrok provider 启停（基础版）
- [x] 隧道状态持久化
- [x] 路由配置文件读写
- [x] CLI: `tunnel start/stop/status/logs(--follow)`, `routes list/add/remove`
- [x] provider 异常退出自动重启策略
- [x] provider stdout/stderr 持久化日志
- [x] 控制面 token 鉴权（Bearer）

## Phase 2: 网关转发

- [x] host/path 路由匹配
- [x] HTTP 反向代理
- [x] WebSocket 透传（基础版）
- [x] 路由级失败切换（primary -> fallback upstream）
- [x] 健康探测（active health check）
- [x] `wss/https` upstream 透传补强（基础支持）

## Phase 3: 接入与产品化

- SoloMesh 外部接入
- Tauri GUI MVP
- 配置热更新
- 运行日志与审计

## Phase 4: 增强

- ngrok provider
- 链接签发（token/code）
- 多租户隔离策略
- 观测性（metrics/tracing）
