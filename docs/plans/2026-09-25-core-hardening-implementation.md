# TunnelMux 核心闭环优化实现计划

**依据：** [2026-09-25-core-hardening-design.md](2026-09-25-core-hardening-design.md)  
**执行方式：** 四个顺序批次；每批独立 commit、验证通过后再进入下一批。

## Batch 0：发布解阻

### 修改文件

- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `scripts/install.sh`
- `scripts/verify-easy-path.sh`（如需统一门禁）
- `docs/RELEASING.md`
- `docs/ARCHITECTURE.md` / `docs/zh/ARCHITECTURE.md`（只同步当前 ownership 描述）

### 步骤

1. 删除 release GUI bundle job 中生成 `externalBin` 的临时 config。
2. 保留 Windows signing config 的合并逻辑，但确保非 Windows/无签名时使用基础 Tauri config。
3. 增加 workflow 静态检查：最终 merged config 不得出现 `externalBin`。
4. CI 执行 Node GUI tests、JS syntax、所有 tracked shell `bash -n`。
5. `install.sh` 找不到 `sha256sum`/`shasum` 时退出，不再继续解包。
6. build/gui job 显式 `contents: read`，publish job 才使用 `contents: write`。
7. 更新发布文档，删除“GUI 打包 sidecar daemon”的描述。

### 测试

- YAML/配置静态检查。
- `bash -n scripts/*.sh`。
- Node test suite。
- 本地 raw archive packaging test。
- tag rehearsal 前运行一次 merged-config smoke。

### 完成条件

- clean checkout 不需要 `crates/tunnelmux-gui/bin/`。
- release workflow 不再生成 sidecar overlay。
- checksum 缺失时安装脚本明确失败。

## Batch 1：认证与 updater 安全

### 修改文件

- `crates/tunnelmux-control-client/src/lib.rs`
- `crates/tunnelmuxd/src/api.rs`
- `crates/tunnelmuxd/src/lib.rs`
- `crates/tunnelmuxd/src/gateway.rs`
- `crates/tunnelmuxd/src/runtime.rs`
- `crates/tunnelmux-gui/src/commands.rs`
- `crates/tunnelmux-gui/src/embedded_daemon.rs`
- `crates/tunnelmux-gui/ui/app.js`
- 相关测试文件

### 步骤

1. 给 control client 增加 base URL host 分类：loopback 才允许 token auto-discovery。
2. `health` 请求不带 token；远端 token 必须显式传入。
3. auth status/unlock/relock 加 bearer gate，保留 access code 作为 unlock body 校验。
4. gateway 放行后删除门禁 Authorization 和 `tunnelmux_access_*` Cookie，保留其它 upstream headers。
5. daemon 忽略并拒绝 API metadata 中的 `providerBinaryPath`。
6. embedded daemon 只通过 `DaemonArgs.cloudflared_bin/ngrok_bin` 传 binary。
7. updater 增加：严格 semver、basename/path containment、SHA-256 必填、最大响应大小、request timeout。
8. native bundle 更新改为打开 release/安装引导，不覆盖 app bundle 内部 binary。
9. 更新错误文案和文档，移除“access code 是 endpoint 唯一凭证”的表述。

### 测试

- remote + token empty 不读取本地 token。
- health 不携带 Authorization。
- forwarded loopback 无 bearer 时 auth endpoint 返回 401。
- HTTP/WS gate credential filtering。
- provider path override rejection。
- updater traversal/missing hash/oversize/timeout rejection。
- 现有 control-client 和 daemon auth 测试全部通过。

## Batch 2：生命周期、route scope 与持久化

### 修改文件

- `crates/tunnelmux-core/src/lib.rs`
- `crates/tunnelmuxd/src/lib.rs`
- `crates/tunnelmuxd/src/api.rs`
- `crates/tunnelmuxd/src/gateway.rs`
- `crates/tunnelmuxd/src/persistence.rs`
- `crates/tunnelmuxd/src/runtime.rs`
- `crates/tunnelmux-gui/src/commands.rs`
- `crates/tunnelmux-gui/ui/app.js`
- `crates/tunnelmux-gui/src/settings.rs`

### 步骤

1. 为 PersistedState 增加 schema version 和旧 route_access migration。
2. route_access 改为 `(tunnel_id, route_id)` key。
3. access Set/List API 增加 tunnel scope；更新 GUI 和 client 调用。
4. 删除/replace/delete tunnel 时清理 access config。
5. route/tunnel ID 做 slug/保留字/控制字符校验。
6. RuntimeState 增加 per-tunnel generation 和 in-flight start 标记。
7. start spawn 返回后检查 generation/shutdown；stop/delete/shutdown 取消 in-flight start。
8. 统一管理 monitor、gateway、SSE、WebSocket task 的取消和等待。
9. state/settings 使用唯一临时文件、sync、atomic rename；Unix 0600。
10. 加 data-dir single-writer/lock 保护。
11. GUI route+gate 失败补偿；gate unknown 保留旧 snapshot。
12. profile delete 在 daemon 不可达时保留 pending cleanup。

### 测试

- 同 route id 跨 tunnel gate 隔离。
- 删除/replace 后 access 不复活。
- start-vs-stop、double-start、spawn-vs-shutdown barrier 测试。
- shutdown 后 gateway 端口可重绑。
- 持久化失败、并发写、权限和损坏恢复测试。
- GUI gate rejection 和 route partial-save 测试。

## Batch 3：CLI、性能和用户体验

### 修改文件

- `crates/tunnelmux-cli/src/main.rs`
- `crates/tunnelmux-cli/src/commands.rs`
- `crates/tunnelmux-cli/src/output.rs`
- `crates/tunnelmuxd/src/gateway.rs`
- `crates/tunnelmuxd/src/runtime.rs`
- `crates/tunnelmuxd/src/api.rs`
- `crates/tunnelmux-gui/ui/app.js`
- `crates/tunnelmux-gui/ui/index.html`
- `crates/tunnelmux-gui/ui/styles.css`
- `crates/tunnelmux-gui/tauri.conf.json`
- CI/release/docs

### 步骤

1. CLI 增加统一 `--tunnel-id`，所有 scoped endpoint 使用同一选择。
2. named Cloudflare readiness 与 quick tunnel 分离。
3. `truncate_cell` 改为 Unicode 边界安全，增加 CJK/emoji 测试。
4. health check 改为 bounded concurrency，完成一项更新一项。
5. gateway 使用 route snapshot，减少每请求全量 clone。
6. SSE 改为增量 tail，并响应 cancellation。
7. GUI 增加 visibility-aware bounded refresh，避免长期陈旧。
8. 恢复 CSP，逐步减少 global bridge 依赖。
9. drawer/confirm 增加 dialog 语义、Escape、focus restore。
10. 更新 sidecar、跨平台 updater、安装路径和版本文档。
11. 增加跨平台 CI、E2E smoke、CSP/accessibility 和 benchmark 检查。

### 测试

- CLI 多 tunnel 读写和跨命令一致性。
- named Cloudflare readiness。
- Unicode/emoji table output。
- health cycle、route snapshot、log rotation benchmark。
- GUI visibility refresh、dialog keyboard、CSP smoke。
- macOS/Windows/Linux 核心 check/test 和 native rehearsal。

## 每批统一门禁

```text
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
node --check crates/tunnelmux-gui/ui/app.js
node --test crates/tunnelmux-gui/ui/app.test.mjs \
  scripts/verify-easy-path.test.mjs \
  scripts/package-local-release-archive.test.mjs
bash -n scripts/*.sh
```

额外要求：

- 每批只 stage 明确路径，禁止 `git add -A`。
- 每批结束检查 `git status --porcelain`。
- 任何门禁失败先修复本批，不进入下一批。
- 每个批次完成后记录 commit、测试结果和剩余风险。
