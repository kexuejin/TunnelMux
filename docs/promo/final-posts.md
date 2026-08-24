# Final launch posts (copy-paste ready)

## 1. Show HN (English)

**Title:**

Show HN: TunnelMux — a GUI-first local tunnel console for cloudflared/ngrok (Rust + Tauri)

**Body:**

TunnelMux is a free, open-source desktop app that replaces the pile of ad-hoc `cloudflared` / `ngrok` commands, route scripts, and half-broken local demos with one local control plane.

What it does:

- Create, start, and stop tunnels (cloudflared quick/named tunnels, ngrok with reserved domains) from a GUI
- Expose several local services per tunnel — frontend + API + docs + webhooks — with host/path routing and health checks
- Protect public routes with per-route access codes (route-scoped cookies), or keep them public
- DeepSeek / SPA preset: mount loopback-protected apps like DeepSeek Harness under `/deepseek` with correct Host/Origin handling and root `/` closed by default
- Built-in updater: checks GitHub Releases, verifies SHA256, installs the raw archive, then Restart Now
- English / 简体中文 UI, in-app auto-updates, diagnostics and logs

It is Rust + Tauri: one daemon + one API power both the GUI and the CLI, so it is also scriptable for CI/CD.

Install (macOS/Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Or grab the `.dmg` / `.msi` / `.deb` from GitHub Releases.

Docs: https://kexuejin.github.io/TunnelMux/
Repo: https://github.com/kexuejin/TunnelMux

I would love feedback on the route/access-gate model and the updater — what is missing for you to switch from your current setup?

---

## 2. Reddit (English, r/selfhosted / r/rust / r/locallama)

**Title:**

TunnelMux — open-source desktop app to expose local services (cloudflared/ngrok) with access gates; great for remote access to DeepSeek Harness / Ollama / Open WebUI

**Body:**

TL;DR: a GUI-first local tunnel console in Rust + Tauri. One app to create/start tunnels, route multiple local services (frontend, API, docs, webhooks), add per-route access codes, and keep root `/` closed by default.

The use case that made me build it: I wanted DeepSeek Harness reachable from my phone. With the **DeepSeek / SPA Preset** it mounts under `/deepseek`, handles Host/Origin correctly for loopback-protected apps, and stays behind an access code.

Install: `curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash`

What would make you switch from `cloudflared` CLI or ngrok?

---

## 3. 掘金 / 公众号 / 知乎（中文）

**标题：**

开源：给本地 AI 工具加远程访问的桌面控制台 TunnelMux（DeepSeek Harness / Ollama / Open WebUI 都能挂）

**正文：**

TunnelMux 是我用 Rust + Tauri 写的「GUI 优先」本地隧道控制台，用来替代散落在终端里的 cloudflared/ngrok 命令、路由脚本和各种临时暴露方案。

它能做什么：

- 一个桌面 App 建隧道、启停隧道、按隧道管理服务
- 多服务 host/path 路由 + 健康检查
- 服务访问门禁：全局默认码 + 每个服务 继承/自定义/公开
- DeepSeek / SPA 预设：一键把 DeepSeek Harness 这类 loopback 保护的 App 挂到子路径，自动处理 Host/Origin 和响应路径重写，根路径 / 默认关闭
- 内置更新：检查 GitHub Releases、校验 SHA256、安装后一键重启
- 英文 / 简体中文界面

典型场景：让 DeepSeek Harness 或 Open WebUI 在外面也能访问。加一个服务、点一下预设，https://你的域名/deepseek 就映射到 http://127.0.0.1:3080，路由还带访问码。

安装（macOS/Linux）：

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

也可以直接下载 Release 里的 .dmg / .msi / .deb。

文档站：https://kexuejin.github.io/TunnelMux/
GitHub：https://github.com/kexuejin/TunnelMux

如果 TunnelMux 帮你省了时间，欢迎点个 ⭐，让更多开发者找到它。
