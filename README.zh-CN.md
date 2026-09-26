# TunnelMux

[English](README.md) | [简体中文](README.zh-CN.md)

![CI](https://github.com/kexuejin/TunnelMux/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/kexuejin/TunnelMux/actions/workflows/release.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Release version](https://img.shields.io/github/v/release/kexuejin/TunnelMux?sort=semver)
![GitHub stars](https://img.shields.io/github/stars/kexuejin/TunnelMux)
![Downloads](https://img.shields.io/github/downloads/kexuejin/TunnelMux/total)

> ⭐ **如果 TunnelMux 帮你省了时间，欢迎点个 Star，让更多开发者找到它。**

**最新版本：** [v0.4.0](https://github.com/kexuejin/TunnelMux/releases/tag/v0.4.0) · [发布说明](docs/releases/v0.4.0.md)

TunnelMux 是一个面向开发者的本地隧道控制台，优先提供 GUI 体验，用来替代散落在终端里的 `cloudflared`、`ngrok`、路由脚本和各种临时命令。

当你的本地开发已经不是“只暴露一个 3000 端口”，而是“前端 + API + 文档 + 回调服务”一起跑时，TunnelMux 用一个本地控制面把隧道启动、服务暴露、Provider 切换和故障排查收拢到一起。

**和本地 AI 工具很配** —— 一条路径即可挂载 DeepSeek Harness、Ollama、Open WebUI，配合内置访问门禁，公开路由也能保持安全。

![TunnelMux 桌面控制台](docs/images/gui-home.png)

## v0.4.0 更新了什么

- **桌面端单进程运行时**：应用在自身进程内托管 daemon，统一持有控制端口和 provider 进程树；如果本机已有 daemon 在运行，则直接接入且退出时不会误杀它。
- **重构后的控制台**：常驻侧边栏 + Overview / Tunnels / Services / Diagnostics / Settings 五个视图，并提供语义化的 Auto / Dark / Light 主题。
- **隧道级控制面**：路由、访问门禁、日志、健康检查、指标和 CLI 操作都明确作用于当前选中的 tunnel，不再默认共享 `primary` 状态。
- **更安全的本机凭据**：GUI 控制 token 和 provider token 迁移到 macOS Keychain、Windows Credential Manager 或 Linux Secret Service；`settings.json` 只保留非敏感配置。
- **默认拒绝的更新器**：支持校验后的 `.tar.gz` / `.zip` raw archive，强制 SHA-256、安全解包，并为原生 `.dmg` / `.msi` / `.deb` 安装提供明确的手动更新路径。

完整范围见 [v0.4.0 发布说明](docs/releases/v0.4.0.md) 和 [CHANGELOG](CHANGELOG.md)。

## 目录

- [v0.4.0 更新了什么](#v040-更新了什么)
- [为什么会需要 TunnelMux](#为什么会需要-tunnelmux)
- [你能得到什么](#你能得到什么)
- [安装](#安装)
- [快速开始](#快速开始)
- [配置与凭据](#配置与凭据)
- [服务访问码门禁](#服务访问码门禁)
- [安全](#安全)
- [macOS 首次打开 FAQ](#macos-首次打开-faq)
- [FAQ](#faq)
- [文档](#文档)
- [仓库结构](#仓库结构)
- [参与贡献](#参与贡献)

## 为什么会需要 TunnelMux

现在的本地分享链路很容易失控：

- vibe coding 一天内就能把一个项目拆成多个本地服务
- `cloudflared` / `ngrok` 命令靠手敲，维护成本越来越高
- host/path 路由散落在 shell history、脚本和 README 片段里
- 出问题时很难判断到底是 daemon、tunnel、route 还是本地服务坏了
- 团队成员很难复现同一套本地暴露配置

TunnelMux 的目标不是再造一个平台，而是把这些本地暴露动作收敛成一个可控的本地控制面。

## 你能得到什么

- 一个覆盖主路径的桌面 GUI：建 tunnel、点启动、加服务
- 一个进程内的 daemon 和一套同时服务 GUI / CLI 的 HTTP 控制面
- 五个聚焦单一任务的视图：Overview、Tunnels、Services、Diagnostics、Settings
- Auto / Dark / Light 外观，以及英文 / 简体中文界面切换
- 面向本地多服务的 host/path 路由能力
- 支持 `cloudflared` 和 `ngrok` 的 provider 配置，并提供应用内安装引导
- 路由、访问门禁、日志、健康检查、指标和 CLI 操作都按 tunnel 隔离
- 需要时可展开的流式诊断、provider 日志和恢复指引
- 服务访问码门禁：支持全局默认码，以及每个服务继承/自定义/公开三种模式
- GUI 控制 token 和 provider token 存入操作系统凭据库
- 内置更新检查：从 GitHub Releases 下载匹配平台的 `.tar.gz` / `.zip` raw archive，强制校验 SHA-256 后安装
- 基于 `config.json` 的路由与健康检查热重载

## GUI 优先的使用方式

TunnelMux 先服务最常见的路径：

1. 创建一个 tunnel
2. 选择 `cloudflared` 或 `ngrok`
3. 点击 `Start Tunnel`
4. 添加一个或多个本地服务
5. 直接分享公网地址

如果后面需要更复杂的能力，同一个应用也支持：

- 常驻侧边栏切换 Overview、Tunnels、Services、Diagnostics、Settings
- 在 Settings → Interface 中选择界面语言和外观；Auto 会跟随系统语言与主题
- 多个 tunnel profile，路由、门禁、日志和健康状态都按 tunnel 一致隔离
- provider 专属配置
- tunnel 重启 / 恢复
- 按需查看流式诊断和日志

## 安装

### 最快路径：原生 GUI 安装包

可以直接从 GitHub Releases 下载：

- macOS：`.dmg`
- Windows：`.msi`
- Linux：`.deb`

同时也提供原始平台压缩包，包含：

- `tunnelmuxd`
- `tunnelmux-cli`
- `tunnelmux-gui`
- `tunnelmux-updater` —— Windows 延迟替换辅助程序

桌面 GUI 也可以在 Settings → App Updates 中检查 GitHub Releases。它会优先读取静态 `tunnelmux-latest.json` release manifest，必要时才 fallback 到 GitHub API。发现匹配当前平台的新 raw `.tar.gz` 或 `.zip` archive 后，会先展示 asset 和必需的 SHA-256，下载到平台应用配置目录下的 updates 目录，并拒绝不安全路径 / 非法版本 / 超限响应，校验通过后启用 **Restart Now**。

自动替换只适用于 raw binary 安装。原生 `.dmg`、`.msi`、`.deb` 安装不会被应用从内部覆盖；更新器会引导你走平台包管理或 GitHub Release 页面。

### 一行命令安装

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

示例：

```bash
# 固定版本
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --version v0.4.0

# 安装到 /usr/local/bin
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash -s -- --prefix /usr/local
```

### 从源码运行

```bash
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmuxd --locked
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmux-cli --locked
```

本地开发：

```bash
cargo run -p tunnelmuxd
cargo run -p tunnelmux-gui
```

## 快速开始

### GUI 路径

1. 安装 `cloudflared` 或 `ngrok`
2. 打开 TunnelMux
3. 创建第一个 tunnel
4. 点击 `Start Tunnel`
5. 添加本地服务地址，例如 `http://127.0.0.1:3000`

GUI 把 daemon **跑在自己的进程里** —— 不拉起任何子进程，控制端口、状态文件与 provider 进程都只有唯一 owner。若配置地址上已有 `tunnelmuxd` 在应答，GUI 会改为连接它，并在你退出时保留它继续运行。退出应用会停止它自己启动的 daemon、隧道与 provider 进程；关闭窗口只是收进托盘。

v0.4 控制台会在侧边栏持续显示当前 tunnel，并把工作拆成 Overview、Tunnels、Services、Diagnostics、Settings。服务改为可横向比较的列表行，path、upstream 和访问模式能直接对齐；Settings 统一承载语言、外观、控制面访问和应用更新。

如果当前所选 provider 还没有安装，TunnelMux 会在启动前拦截问题，给出 provider 专属提示，并提供适合当前平台的安装动作和可复制的兜底命令，避免直接落到原始的进程启动报错。

### CLI 路径

```bash
git clone https://github.com/kexuejin/TunnelMux.git
cd TunnelMux

cargo run -p tunnelmuxd -- \
  --listen 127.0.0.1:4765 \
  --gateway-listen 127.0.0.1:18080

cargo run -p tunnelmux-cli -- --tunnel-id primary routes add \
  --id app-web \
  --upstream-url http://127.0.0.1:3000 \
  --path-prefix /app

cargo run -p tunnelmux-cli -- --tunnel-id primary tunnel start \
  --provider cloudflared \
  --target-url http://127.0.0.1:18080 \
  --auto-restart
```

`--tunnel-id` 会一致作用于 tunnel 状态、路由、日志、健康、指标和 dashboard 命令。为兼容已有脚本，省略时仍默认使用 `primary`。

## 适合哪些场景

TunnelMux 很适合这些本地开发场景：

- 前端走一个路径，API 走另一个路径
- 文档、Webhook 回调、本地工具挂在同一个 tunnel 下
- 想用稳定的 Cloudflare named tunnel，或者临时 quick tunnel
- 默认只需要一个 tunnel，后面再逐步增加多个 tunnel profile

它不是生产环境边缘网关，也不是云平台。它更像是本地分享链路上的控制层，用来降低使用成本。

## macOS 首次打开 FAQ

当前原生 GUI 安装包可能仍是未签名状态，因此第一次启动时，macOS 可能会弹出 Gatekeeper 提示。

### “App 已损坏，无法打开”

如果你确认下载来源可信，可以按下面操作：

1. 在 Finder 中找到应用
2. 右键 `TunnelMux.app`
3. 点击 `打开`
4. 在系统弹窗里确认

如果仍然被拦截，再到：

- `系统设置` → `隐私与安全性`
- 在页面底部找到被拦截的应用提示
- 点击 `仍要打开`

### “无法验证开发者”

优先还是走同一套流程：

1. 右键应用
2. 选择 `打开`
3. 在弹窗里确认

如果还是不行：

- `系统设置` → `隐私与安全性`
- 点击 `仍要打开`

### 最后手段：移除 quarantine

只在你确认应用来源可信时再执行：

```bash
xattr -dr com.apple.quarantine /Applications/TunnelMux.app
```

更多发布与打包说明见 [docs/RELEASING.md](docs/RELEASING.md)。

## FAQ

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
不会。自动替换只针对校验过的 raw `.tar.gz` / `.zip` binary 安装。原生 bundle 会引导到平台包管理流程或 GitHub Release 页面，应用不会改写自身 app bundle 内的文件。

**如何从 CLI 操作非默认 tunnel？**
在子命令前传全局参数 `--tunnel-id <id>`。路由、访问门禁、日志、健康、指标、状态和 dashboard 调用都会指向该 tunnel；省略时保持旧的 `primary` 默认值。

**TunnelMux 支持哪些平台？**
macOS（Intel 与 Apple Silicon）、Windows、Linux。GitHub Releases 为三者提供 raw archive 和原生安装包（`.dmg` / `.msi` / `.deb`）。

**如何把 TunnelMux 界面切换成中文？**
使用顶部或 Settings → Interface 的语言选择器，选择 **简体中文**；Auto 跟随系统语言，选择会跨启动保留。

**如何在只暴露子路径时保持根路径 `/` 关闭？**
不要添加 path 为 `/` 的服务。每个服务行都会显示根路径 `/` 是暴露还是关闭，DeepSeek / SPA 预设默认保持根路径关闭。

## 配置与凭据

daemon 文件：

- `~/.tunnelmux/config.json` — 声明式路由与健康检查配置
- `~/.tunnelmux/state.json` — daemon 维护的运行时快照
- `~/.tunnelmux/api-token` — 自动生成的控制面 bearer token（Unix 下仅 owner 可读写）
- `~/.tunnelmux/provider.log` — provider 的 stdout/stderr，超过 16 MiB 轮转为 `provider.log.1…3`

daemon 会轮询 `config.json`，应用路由和健康检查变更时不需要重启。状态、设置和 token 都通过唯一临时文件 + 原子 rename 写入；写失败会明确报错，而不会被当成保存成功。

GUI 的非敏感偏好保存在操作系统应用配置目录下的 `settings.json` 中，例如 macOS 的 `~/Library/Application Support/com.tunnelmux.gui/settings.json`。控制 bearer token、Cloudflare tunnel token 和 ngrok authtoken 单独存入系统凭据库；旧版明文字段会在加载时迁移并删除。

token 与 provider 日志都放在状态文件旁边：用 `--data-file /tmp/scratch/state.json` 启动时，token 落在 `/tmp/scratch/api-token`，绝不会碰到生产 daemon 发出去的 token。路径与大小可分别用 `--api-token-file`、`--provider-log-file`、`--provider-log-max-bytes`（`0` 表示不轮转）、`--provider-log-max-files`（`0` 表示不留备份）覆盖。

## 服务访问码门禁

公网 route 可以在进入 upstream 服务之前先要求访问码。在 Settings → Default service access 中可以配置全局默认服务访问码；每个服务也可以在编辑抽屉里选择：

- `Inherit default gate`：有默认码时继承默认码
- `Use custom service code`：使用该服务自己的访问码
- `Always public`：即使配置了默认码，该服务也保持公开

默认门禁存储在 `~/.tunnelmux/state.json` 的 `default_route_access`，服务覆盖存储在 `route_access`。从 v0.4.0 开始，每条覆盖按 **tunnel + route** 联合标识，因此不同 tunnel 中的同名 route 也能保持独立门禁；旧版仅按 route 存储的状态会在加载时迁移。浏览器解锁后使用类似 `tunnelmux_access_<route_id>` 的 route 级 cookie，因此一个服务解锁不会打开其它 route。

对于 DeepSeek Harness 这类挂载在子路径下的 SPA，可以在服务编辑器里点击 **DeepSeek / SPA Preset**。它会设置 path mount、关闭 Host forwarding、打开 response path rewrite，并提示根路径 `/` 仍保持关闭，除非另一个服务显式暴露它。每张服务行也提供 **Test**，用于检查公网 route 和 upstream 状态。

## 安全

控制面 API（`127.0.0.1:4765`）使用 bearer token 认证。`--control-auth` 可选 `require`（默认）、`optional` 或 `off`。在 `require` 模式下，所有受保护 endpoint 都要求有效 token；如果尚未配置，daemon 会在状态文件旁自动生成 `api-token`，默认即 `~/.tunnelmux/api-token`。

CLI、GUI、共享 control client 以及兼容的远程集成，**只在控制面 base URL 解析为 loopback 时**自动读取这个 token。远端 daemon 必须通过 `--token` 或 `TUNNELMUX_API_TOKEN` 显式提供 token；本机凭据不会被附加到远端请求。`GET /v1/health` 有意保持免认证，并且客户端不会发送控制 token。

你也可以用便于输入的访问码**解锁 loopback**（`--unlock-code <CODE>`；未设置时自动轮换，默认窗口 4h，可用 `--unlock-window <ms>` 调整）。解锁后，本机请求可以不带 token；非 loopback（例如桥接）访问仍要求 bearer token。auth status / unlock / relock endpoint 本身仍要求控制 bearer token——访问码只负责解锁会话，不能替代 endpoint 认证。可使用 `tunnelmux unlock <code>` / `tunnelmux unlock --show-code` / `--relock`，或在 GUI 的 Settings → Control-plane access 中操作。

v0.4.0 还增加了这些边界：

- gateway 转发给 upstream 前，会移除 route gate 使用的 `Authorization` 和 `tunnelmux_access_*` Cookie，同时保留其它正常请求头。
- provider 可执行文件只来自 daemon 启动配置；HTTP API 不再接受单次请求级别的 provider binary 覆盖。
- 更新器在改动任何二进制前，都会校验合法 semver、单一安全 asset 名、限定时间内的大小/响应，以及匹配的 SHA-256。
- GUI 控制 token 和 provider token 存入操作系统凭据库，不再以明文留在 settings 中。

## 文档

- [文档站点](https://kexuejin.github.io/TunnelMux/)
- [架构说明](docs/zh/ARCHITECTURE.md)
- [API](docs/zh/API.md)
- [第三方集成](docs/zh/INTEGRATION.md)
- [集成模板](docs/zh/INTEGRATION-TEMPLATES.md)
- [路线图](docs/zh/ROADMAP.md)
- [发布流程](docs/RELEASING.md)
- [v0.4.0 发布说明](docs/releases/v0.4.0.md)
- [更新日志](CHANGELOG.md)
- [English docs](docs/index.md)

## 仓库结构

- `crates/tunnelmux-core` — 共享领域模型与协议类型
- `crates/tunnelmux-control-client` — GUI / CLI 共用的 HTTP 控制客户端
- `crates/tunnelmuxd` — daemon 运行时与控制面 API，同时提供 library 和精简的独立 binary
- `crates/tunnelmux-gui` — Tauri 桌面控制台、系统凭据库、updater 集成和 Windows 延迟替换辅助程序
- `crates/tunnelmux-cli` — CLI 客户端与运维命令
- `scripts/install.sh` — macOS / Linux 安装脚本

## 参与贡献

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
