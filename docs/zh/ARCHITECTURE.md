---
title: TunnelMux 架构
description: TunnelMux 的组件划分、数据面/控制面分离与设计原则。
---

# TunnelMux 架构

## 定位

TunnelMux 是一个独立的本地基础设施组件，提供：

- 隧道生命周期控制（`start` / `stop` / `status`）
- 反向代理路由（`host/path` → 本地上游）
- 面向自动化与外部集成的本地控制面 API

TunnelMux 刻意保持与具体产品无关。

## 核心组件

### 1. `tunnelmuxd`（Rust daemon）

职责：

- 从 `config.json` 加载声明式配置，并热重载路由 / 健康检查设置
- 暴露控制面 API（默认：`127.0.0.1:4765`）
- 管理 provider 进程（`cloudflared`、`ngrok`）
- 存储并提供运行时状态与路由配置
- 以退避策略监督 provider 生命周期并自动重启
- 暴露 provider 日志与 SSE 日志流
- 暴露上游健康快照与流

`tunnelmuxd` 同时构建为**库与二进制**：库导出 `DaemonArgs`、`start()`、`serve()`，
二进制只是它们的薄 CLI 包装。无头部署运行二进制；桌面应用链接该库，把同一个
daemon 跑在自身进程内 —— 这正是它能成为控制端口、状态文件、api token 与
provider 子进程唯一 owner 的原因。

### 2. 网关数据面

职责：

- 接收来自活动隧道端点的入口流量
- 按 `host/path` 匹配并转发请求
- 支持 HTTP 与 WebSocket 升级转发
- 应用主 / 备 failover 策略
- 利用活跃健康检查信号优先选择健康目标

### 3. `tunnelmux-control-client`

职责：

- 提供共享的 Rust HTTP 客户端，用于非流式控制面操作
- 集中处理 bearer token 与结构化 API 错误解码
- 保持 CLI 与 GUI 请求行为一致

### 4. `tunnelmux-cli`

职责：

- 默认的运维控制面
- 调用 daemon API 完成生命周期、路由、诊断与设置操作
- 提供人类可读与机器可读两种输出模式
- 让流式 / 日志流程适合终端工作流

### 5. `tunnelmux-gui`（Tauri 桌面壳）

职责：

- 为运维人员提供本地操作控制台
- 通过链接 `tunnelmuxd` 库，**在进程内托管 daemon**，使应用成为隧道生命周期的唯一 owner
- 把它实际探测到的连接设置（daemon `base_url` 与 token）落盘，冷启动即可连回同一端口
- 调用委托给共享控制客户端的 Tauri 命令
- 呈现仪表盘、隧道控制、路由 CRUD、诊断与托盘图标

所有权规则：

- 若配置地址上已有 daemon 在应答，GUI 直接接管（adopt）且永不停止它 —— 这是 CLI 所用
  无头 `tunnelmuxd` 的路径
- 否则 GUI 启动内嵌 daemon，并在退出时停止它，同时终止它自己拥有的 provider 进程

GUI 刻意**不**做：

- 打包或拉起独立的 `tunnelmuxd` sidecar 二进制
- 在配置的 daemon 不可达时回落到 `PATH` 上的 daemon
- 在隧道存在之前自动拉起诊断订阅

## 设计原则

- 单隧道、多个本地服务路由
- 清晰的 控制面 / 数据面 分离
- API-first 的集成面
- 本地优先安全（loopback 绑定 + 可选 bearer token）
- 每个资源只有一个 owner（一个控制端口、一份状态文件、一组 provider）
- 显式的配置 / 运行时分离（`config.json` 期望状态 vs `state.json` 运行时快照）
- 与调用方无关的设计（不内嵌业务适配器）
- 客户端平等模型（CLI 与 GUI 通过同一 daemon API 对等访问）
