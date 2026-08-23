# 推广文案 — 中文（掘金 / 公众号 / 知乎）

## 标题候选

- 开源：给本地 AI 工具加远程访问的桌面控制台 TunnelMux（DeepSeek Harness / Ollama / Open WebUI 都能挂）
- 还在敲 cloudflared 命令吗？这个 Rust + Tauri 开源工具把本地隧道做成了桌面 App
- 5 分钟给 DeepSeek Harness 配好远程访问，根路径还默认关闭

## 正文

TunnelMux 是我用 Rust + Tauri 写的一个「GUI 优先」本地隧道控制台。

**痛点：** 当你的本地开发从“只暴露一个 3000 端口”变成“前端 + API + 文档 + 回调服务一起跑”时，本地分享就成了一堆临时 `cloudflared` / `ngrok` 命令、路由脚本和修不完的 demo。

**它能做什么：**

- 一个桌面 App 就能建隧道、启停隧道、按隧道管理服务
- 支持 cloudflared quick / named tunnel 和 ngrok 保留域名
- 多服务 host/path 路由 + 健康检查
- 服务访问门禁：全局默认码 + 每个服务继承 / 自定义 / 公开三种模式（路由级 Cookie，互不干扰）
- 内置更新：检查 GitHub Releases、校验 SHA256、安装后一键重启
- 英文 / 简体中文界面
- DeepSeek / SPA 预设：一键把 DeepSeek Harness 这类 loopback 保护的子路径 App 挂到公网路径，自动处理 Host/Origin 和响应路径重写

**一个典型场景：** 想让 DeepSeek Harness 或 Open WebUI 能在外面访问。加一个服务、点一下预设，`https://你的域名/deepseek` 就映射到 `http://127.0.0.1:3080`——根路径 `/` 保持关闭，路由还可以加访问码。

**安装（macOS / Linux）：**

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

也可以直接下载 Release 里的 `.dmg` / `.msi` / `.deb`。

一个 daemon + 一个 API，同时服务 GUI 和 CLI。欢迎反馈，尤其是路由/门禁模型和更新器。

## 文末 CTA

> 如果 TunnelMux 帮你省了时间，欢迎点个 ⭐ Star，让更多开发者找到它。

> GitHub: https://github.com/kexuejin/TunnelMux
