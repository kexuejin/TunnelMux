---
title: 第三方集成指南
description: 外部平台如何通过控制面 API 与 TunnelMux 集成。
---

# 第三方集成指南

本文档描述任何外部平台如何与 TunnelMux 集成。

可直接改写的 Bash、Node.js、Python 示例见 [集成模板](INTEGRATION-TEMPLATES.md)。

## 集成边界

- TunnelMux 拥有隧道生命周期与网关路由
- 外部平台作为 API 客户端
- 业务逻辑留在外部平台，而不是写进 TunnelMux

## 推荐的集成模式

### 1. 在外部平台中配置

存储：

- `TUNNELMUX_BASE_URL`（默认：`http://127.0.0.1:4765`）
- `TUNNELMUX_API_TOKEN`（可选但推荐）

### 2. 生命周期流程

典型启动流程：

1. `GET /v1/tunnel/status`
2. 未运行时调用 `POST /v1/tunnel/start`
3. 确保路由存在（`POST /v1/routes` 或 `POST /v1/routes/apply`）

### 3. 路由策略

推荐顺序：

- 稳定的多服务映射优先用 host 路由
- host 分配受限时使用 path 路由
- 配置备用上游以实现优雅 failover

### 4. 运维模型

外部平台可以：

- 轮询 `GET /v1/dashboard` 获取汇总运行时快照
- 订阅 SSE 端点获取实时状态更新
- 通过 `GET /v1/tunnel/logs` 或 `/stream` 读取 provider 日志

### 5. 安全基线

- 尽量把控制 API 绑定到 loopback
- 非开发环境开启 API token
- 限制 token 只分发给可信的服务组件

## 迁移蓝图（用于已有应用内隧道逻辑）

1. 让 TunnelMux 与现有隧道实现并行运行
2. 把路由生命周期操作迁移到 TunnelMux API
3. 验证期间保留旧逻辑作为回退
4. 切换完成后移除应用内 provider 进程所有权
