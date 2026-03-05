# TunnelMux

TunnelMux 是一个独立的“隧道 + 转发网关”服务。

![CI](https://github.com/kexuejin/TunnelMux/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/kexuejin/TunnelMux/actions/workflows/release.yml/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)

目标：
- 一个隧道暴露多个本机服务（不同端口）
- CLI 作为默认操作面
- GUI 通过 Tauri 连接同一控制 API
- SoloMesh 作为客户端接入，而不是内置隧道逻辑

## 安装方式

### 方式 1：GitHub Releases（二进制，推荐）

在 Releases 页面下载对应平台压缩包，解压后获得：

- `tunnelmuxd`
- `tunnelmux-cli`

把二进制放到 `PATH` 即可全局使用。

### 方式 2：源码安装（Cargo）

```bash
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmuxd --locked
cargo install --git https://github.com/kexuejin/TunnelMux tunnelmux-cli --locked
```

开发者本地安装（当前仓库）：

```bash
cargo install --path crates/tunnelmuxd --force
cargo install --path crates/tunnelmux-cli --force
```

## 项目结构

- `crates/tunnelmux-core`: 共享模型与核心类型
- `crates/tunnelmuxd`: 独立内核服务（daemon）
- `crates/tunnelmux-cli`: CLI 客户端
- `docs/`: 架构、API、集成文档

## 快速开始

```bash
git clone https://github.com/kexuejin/TunnelMux.git
cd TunnelMux
cargo check
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmuxd -- --listen 127.0.0.1:4765 --gateway-listen 127.0.0.1:18080 --max-auto-restarts 10 --health-check-interval-ms 5000 --health-check-timeout-ms 2000 --health-check-path /healthz
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- status
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- routes add --id solomesh --upstream-url http://127.0.0.1:3000 --fallback-upstream-url http://127.0.0.1:3001 --health-check-path /healthz --host solomesh.local
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- routes list
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- upstreams health                    # 默认表格
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- upstreams health --json             # JSON 输出
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- upstreams health --watch --interval-ms 2000
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- tunnel logs --lines 100
TUNNELMUX_API_TOKEN=dev-token cargo run -p tunnelmux-cli -- tunnel logs --follow --lines 50
curl -H 'Host: solomesh.local' http://127.0.0.1:18080/
```

## 当前状态

当前为 Phase 1（最小可用）：
- 已有 daemon 控制 API：健康检查、隧道启动/停止/状态、路由增删查
- 已有 CLI 命令：`status`、`tunnel start|stop|logs(--follow)`、`routes list|add|remove`
- 已有状态持久化（默认 `~/.tunnelmux/state.json`，可通过 `--data-file` 覆盖）
- 已支持启动 cloudflared / ngrok 进程并解析公开 URL
- 已支持 provider 异常退出自动重启（指数退避：1/2/4/8/16/32 秒，上限由 `--max-auto-restarts` 控制）
- 已支持 provider stdout/stderr 持久化到日志文件（默认 `~/.tunnelmux/provider.log`，可通过 `--provider-log-file` 覆盖）
- 已支持日志读取 API：`GET /v1/tunnel/logs?lines=200`
- 已支持日志流 API（SSE）：`GET /v1/tunnel/logs/stream?lines=50&poll_ms=1000`
- 已支持控制面 token 鉴权（daemon `--api-token` 或 `TUNNELMUX_API_TOKEN`，客户端 Bearer Token）
- 已支持内置 HTTP + WebSocket 反向代理网关（按 host/path 路由到不同 upstream）
- 已支持路由级失败切换（主上游失败时自动切到 `fallback_upstream_url`）
- 已支持主动健康探测（周期探测 upstream 健康；当主上游不健康且 fallback 健康时优先尝试 fallback）
- 主动健康探测支持自定义探测路径（`--health-check-path`，默认 `/`）
- 已支持上游健康状态查询（`GET /v1/upstreams/health` + `tunnelmux upstreams health`）
- 已支持路由级探测路径覆盖（`routes add --health-check-path`，未设置时回落到 daemon 全局 `--health-check-path`）
- 上游健康查询 CLI 默认表格输出，`--json` 切换机器可读格式
- 上游健康查询支持 watch 模式（`tunnelmux upstreams health --watch`，`--interval-ms` 范围 `200..=60000`）

当前限制：
- WebSocket 已支持 `wss/https` 上游透传并有基础 e2e 测试，仍需更完整压测与兼容性覆盖
- 尚未实现多租户隔离（后续阶段）

## 文档索引

- [架构设计](docs/ARCHITECTURE.md)
- [控制 API 草案](docs/API.md)
- [SoloMesh 接入方式](docs/SOLOMESH-INTEGRATION.md)
- [开发路线图](docs/ROADMAP.md)
- [发布流程](docs/RELEASING.md)

## 维护者发布

首次将本地仓库发布到 GitHub：

```bash
git remote add origin git@github.com:<your-org-or-user>/TunnelMux.git
git branch -M main
git push -u origin main
```

发布新版本（自动构建多平台 release 包）：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 开源协作

- [Contributing](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
