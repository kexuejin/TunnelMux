---
title: 常见问题（FAQ）
description: TunnelMux 常见问题：如何暴露本地服务、与 ngrok/cloudflared 的对比、DeepSeek Harness/Ollama/Open WebUI、访问码、平台支持、中文界面、根路径关闭。
---

# 常见问题（FAQ）

**如何用一个公网 URL 暴露本地服务？**

创建一个隧道（`cloudflared` 或 `ngrok`）并启动，然后添加一个指向本地地址（例如 `http://127.0.0.1:3000`）的服务。TunnelMux 会在服务行上显示公网 URL 和路由状态。

**TunnelMux 和直接用 `cloudflared` / `ngrok` 命令行有什么不同？**

命令行适合单条隧道。TunnelMux 是桌面控制面：多服务 host/path 路由、健康检查、每路由访问门禁、provider 日志、诊断和内置更新器都在一个 GUI 里，底层同一个 daemon/API 也同时服务 CLI。

**可以暴露 DeepSeek Harness、Ollama 或 Open WebUI 吗？**

可以。添加服务后使用 **DeepSeek / SPA 预设**：把 loopback 保护的 App 挂到子路径（例如 `/deepseek` → `http://127.0.0.1:3080`），关闭原始 Host header 让上游看到 loopback Host，重写响应路径，并默认保持根路径 `/` 关闭。

**如何用访问码保护公网路由？**

在 Settings → Default service access 设置默认服务访问码，或对每个服务选择 继承 / 自定义 / 公开。访客用访问码解锁一次，TunnelMux 按路由写入 Cookie，其它路由不受影响。

**GUI 控制 token 和 provider token 存在哪里？**
保存在操作系统的凭据库中，服务名为 `com.tunnelmux.gui`：macOS Keychain、Windows Credential Manager 或 Linux Secret Service 集成。`settings.json` 只保留非敏感配置。Linux 上需要可用的 Secret Service / keyring 会话才能持久化 GUI token。

**应用内更新器会替换 `.dmg`、`.msi` 或 `.deb` 安装吗？**
不会。自动替换只针对校验过的 raw `.tar.gz` / `.zip` binary 安装。原生 bundle 会引导到平台包管理流程或 GitHub Release 页面。

**如何从 CLI 操作非默认 tunnel？**
在子命令前传全局参数 `--tunnel-id <id>`。路由、访问门禁、日志、健康、指标、状态和 dashboard 调用都会指向该 tunnel；省略时保持旧的 `primary` 默认值。

**TunnelMux 支持哪些平台？**

macOS（Intel 与 Apple Silicon）、Windows、Linux。GitHub Releases 为三者提供 raw archive 和原生安装包（`.dmg` / `.msi` / `.deb`）。

**如何把 TunnelMux 界面切换成中文？**

使用顶部或 Settings → Interface 的语言选择器，选择 **简体中文**；Auto 跟随系统语言，选择会跨启动保留。

**如何在只暴露子路径时保持根路径 `/` 关闭？**

不要添加 path 为 `/` 的服务。每个服务行都会显示根路径 `/` 是暴露还是关闭，DeepSeek / SPA 预设默认保持根路径关闭。
