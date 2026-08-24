---
title: TunnelMux 路线图
description: TunnelMux 的发展阶段与计划。
---

# TunnelMux 路线图

## 阶段 0：基础（已完成）

- [x] Rust workspace 初始化
- [x] daemon + CLI 基础命令
- [x] 初始架构与 API 文档

## 阶段 1：最小可用产品（已完成）

- [x] provider 进程生命周期（`cloudflared` / `ngrok`）基础
- [x] 隧道状态持久化
- [x] 路由配置持久化
- [x] CLI 操作：`tunnel` 与 `routes`
- [x] provider 自动重启策略
- [x] provider 日志持久化与流式
- [x] token 保护的控制面 API

## 阶段 2：网关与路由（已完成）

- [x] host/path 路由匹配
- [x] HTTP 反向代理
- [x] WebSocket 代理
- [x] 路由级主 / 备 failover
- [x] 活跃健康检查
- [x] `wss/https` 上游支持基础

## 阶段 3：产品化与生态集成（已完成）

- [x] 通用第三方集成模板
- [x] GUI MVP（Tauri）
- [x] GUI 诊断工作区
- [x] 配置热重载
- [x] 运维审计与诊断

## 阶段 4：高级能力

- [ ] 加固的多租户隔离模型
- [ ] 签名短链管理 API
- [ ] provider 插件模型与扩展 API
- [ ] 可观测性（metrics、tracing、profiling）
