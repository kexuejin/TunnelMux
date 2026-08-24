# 掘金 / 公众号 — 6 步图文教程（中文）

## 标题

一个桌面 App 暴露你的本地服务（含 DeepSeek Harness）：TunnelMux 上手指南

## 封面提示

用仓库的 social preview（`.github/social-preview.png`）或一张新 GUI 截图作为封面。

## 引言

本地开发早就不只是“暴露一个 3000 端口”了——前端、API、文档，可能还有 DeepSeek Harness / Open WebUI 这类本地 AI 服务。TunnelMux 是一个免费开源的（Rust + Tauri）桌面应用，把这些全部收进一个 GUI。这篇指南用 6 步带你走通主流程。

## 前置条件

- macOS / Windows / Linux
- 安装 `cloudflared` 或 `ngrok`（也可以在应用内按提示安装）
- 一个要暴露的本地服务，例如 `http://127.0.0.1:3000`

## 第 1 步：安装 TunnelMux

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

或者直接从 GitHub Releases 下载安装包（macOS `.dmg` / Windows `.msi` / Linux `.deb`）。

<!-- 截图：首次启动的主窗口 -->

## 第 2 步：创建第一个隧道

点击 **Create Tunnel**，起个名字，选择 provider，网关目标保持默认即可。配置存在本地，之后随时可改。

<!-- 截图：隧道创建表单 -->

## 第 3 步：启动隧道

点击 **Start Tunnel**。运行后能看到公网 URL，状态变绿。分享前记得先加服务。

<!-- 截图：运行中的隧道与公网 URL -->

## 第 4 步：添加服务

点击 **Add Service**：

- **Local Service URL**：例如 `http://127.0.0.1:3000`
- **Public Path**：例如 `/app`
- **Service enabled** 保持开启

保存后服务卡片会显示暴露方式、健康状态和门禁状态。

<!-- 截图：服务编辑抽屉 -->

## 第 5 步：对 loopback 保护的 App 使用 DeepSeek / SPA 预设

如果你的上游是校验 Host/Origin 的子路径 SPA（DeepSeek Harness 就是典型例子），在服务编辑器里点一下 **DeepSeek / SPA Preset**，它会自动：

- 使用路径挂载（`/deepseek`）
- 关闭 Host 转发（让上游看到 loopback Host，而不是你的公网域名）
- 开启响应路径重写
- 提示根路径 `/` 默认保持关闭

这样 `https://你的域名/deepseek` 就能干净地映射到 `http://127.0.0.1:3080`。

<!-- 截图：已应用 DeepSeek 预设 -->

## 第 6 步：分享一个受保护的 URL

在 **Settings → Default service access** 设置默认访问码，或对每个服务选择 **继承 / 自定义 / 公开**。之后访问公网路由会先要求输入一次访问码，然后按路由写 Cookie，互不干扰。

分享前用服务卡片上的 **Test** 验证公网路由和上游健康。

<!-- 截图：公网访问门禁页 -->

## 小结

安装 → 建隧道 → 启动 → 加服务 → 用预设 → 分享受保护 URL。

如果这篇文章帮到你，欢迎到仓库点个 ⭐：https://github.com/kexuejin/TunnelMux

## 作者备注

- 把每处 `<!-- 截图：... -->` 替换为真实截图（建议 1600px 宽）。
- `deepseek` 这个例子如果与你的受众不符，换成任意本地 Web 应用即可。
