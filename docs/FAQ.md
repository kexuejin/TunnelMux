---
title: FAQ
description: TunnelMux 常见问题：如何暴露本地服务、与 ngrok/cloudflared 的对比、DeepSeek Harness/Ollama/Open WebUI、访问码、平台支持、中文界面、根路径关闭。
---

# FAQ

**How do I expose a local service with a public URL?**

Create a tunnel (`cloudflared` or `ngrok`), start it, then add a service pointing at your local URL (for example `http://127.0.0.1:3000`). TunnelMux shows the public URL and the routing state on each service card.

**How is TunnelMux different from using the `cloudflared` or `ngrok` CLI directly?**

The CLI is great for one tunnel at a time. TunnelMux is a desktop control plane: multi-service host/path routing, health checks, per-route access gates, provider logs, diagnostics, and an in-app updater — in one GUI, with the same daemon/API powering the CLI.

**Can I expose DeepSeek Harness, Ollama, or Open WebUI?**

Yes. Add a service, then use the **DeepSeek / SPA Preset**. It mounts loopback-protected apps under a path (for example `/deepseek` → `http://127.0.0.1:3080`), keeps the original Host header off so the app sees a loopback Host, rewrites response paths, and leaves root `/` closed by default.

**How do I protect my public tunnel routes with an access code?**

Set a default service access code under Settings → Default service access, or choose per-service inherit / custom / public. Visitors unlock each route with the code once; TunnelMux stores a route-scoped cookie so other routes stay unaffected.

**Which platforms does TunnelMux support?**

macOS (Intel + Apple Silicon), Windows, and Linux. GitHub Releases ships raw archives and native installers (`.dmg`, `.msi`, `.deb`) for all three.

**How do I switch the TunnelMux UI to Chinese?**

Use the language selector in the header or Settings → Interface and choose **简体中文**. Auto follows your system language and the choice is remembered between launches.

**How do I keep root `/` closed while exposing a subpath?**

Do not add a service with path `/`. Each service card shows whether root `/` is exposed or stays closed, and the DeepSeek / SPA preset keeps root closed by default.
