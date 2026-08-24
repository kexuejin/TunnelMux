---
title: TunnelMux（本地隧道控制台）
description: 用桌面 GUI 暴露本地服务的隧道控制台，支持 DeepSeek Harness / Ollama / Open WebUI。
---

# TunnelMux（本地隧道控制台）

TunnelMux 是一个 **GUI 优先的本地隧道控制台**（Rust + Tauri），用来替代散落在终端里的 `cloudflared` / `ngrok` 命令、路由脚本和各种临时暴露方案。

启动一条隧道，把本地服务——API、文档、Webhook，以及 DeepSeek Harness、Ollama、Open WebUI 这类本地 AI 工具——从一个桌面应用里暴露出去，并配好多服务路由、健康检查和每路由访问门禁。

## 安装

=== "macOS / Linux"

    ```bash
    curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
    ```

=== "原生安装包"

    从 [GitHub Releases](https://github.com/kexuejin/TunnelMux/releases) 下载最新的 `.dmg` / `.msi` / `.deb`。

## 快速开始

1. 创建隧道（`cloudflared` 或 `ngrok`）
2. 启动隧道
3. 添加本地服务（例如 `http://127.0.0.1:3000`）
4. 分享公网 URL

## 特性

- 桌面 GUI 与 CLI 共享同一个 daemon / API
- 多服务 host/path 路由 + 健康检查
- 服务访问门禁：默认码 + 每个服务 继承 / 自定义 / 公开
- **DeepSeek / SPA 预设**：一键挂载 loopback 保护的 App（如 DeepSeek Harness）
- 内置更新器：SHA256 校验后安装，一键重启
- 英文 / 简体中文界面

## 文档

- [架构](../ARCHITECTURE.md)
- [API](../API.md)
- [集成](../INTEGRATION.md)
- [集成模板](../INTEGRATION-TEMPLATES.md)
- [路线图](../ROADMAP.md)

!!! tip "本地 AI 工具"
    使用 **DeepSeek / SPA 预设** 一键把 DeepSeek Harness、Ollama、Open WebUI 挂到公网路径，并默认保持根路径 `/` 关闭。

> 如果 TunnelMux 帮你省了时间，欢迎到 [GitHub](https://github.com/kexuejin/TunnelMux) 点个 ⭐。

---

[English](https://kexuejin.github.io/TunnelMux/)
