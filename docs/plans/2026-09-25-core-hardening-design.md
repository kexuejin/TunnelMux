# TunnelMux 核心闭环优化设计

**日期：** 2026-09-25  
**状态：** approved by owner  
**范围：** 兼容式增量修复，分四批完成发布、认证/凭据、生命周期/一致性、GUI/updater/CLI 与测试门禁。

## 1. 背景与目标

当前 `main` 已完成 daemon 进程内化、日志轮转、token 路径和 access cookie TTL 等近期修复，但审计仍发现一个发布阻断，以及多 tunnel 隔离、认证边界、嵌入式生命周期、持久化和 updater 的高风险缺口。

本轮目标不是重写 daemon，而是在保持现有 API/用户流程可迁移的前提下，形成可验证的核心闭环：

1. clean checkout 可以构建和发布，不再依赖已删除的 sidecar。
2. 本机 token、远端控制面、route gate 和 provider executable 之间有明确边界。
3. route access、持久化和 tunnel 生命周期在多 tunnel/失败重试场景下保持一致。
4. GUI、CLI 和 updater 对失败状态有可恢复、可解释的行为。
5. 每一批都有自动化回归门禁，避免只靠人工 smoke 发现问题。

## 2. 非目标

- 本轮不实现 provider plugin model。
- 本轮不实现完整 Unix socket 控制面；auth endpoint 先改为 bearer gate，后续再评估 IPC。
- 本轮不实现 Windows zip updater；Windows native installer 走手工/包管理器引导。
- 本轮不把 OS keychain 作为所有 secret 的强制迁移前提；先完成权限、原子写和泄漏边界修复。
- 不在本轮引入大规模 actor/事务存储重构。

## 3. 核心设计

### 3.1 发布和 ownership

- 删除 release workflow 中重新生成 `bundle.externalBin` 的 overlay。
- GUI 只依赖 `tunnelmuxd` library，发布配置不得重新引入 sidecar。
- 增加 merged-config 静态检查和 clean-build rehearsal。
- build/gui job 使用只读仓库权限，只有 publish job 拥有写权限。
- 安装脚本在没有可信 checksum verifier 时 fail-closed。

### 3.2 认证和凭据

- control client 仅在 base URL 解析为 loopback 时自动读取 `~/.tunnelmux/api-token`；远端必须显式提供 token。
- `GET /v1/health` 不携带 Authorization。
- `auth/status`、`auth/unlock`、`auth/relock` 必须带 control bearer token；访问码只负责解锁，不再作为 endpoint 的唯一信任依据。
- gateway 放行后移除用于 route gate 的 `Authorization` 和 `tunnelmux_access_*` Cookie，再转发其它 upstream headers。
- API 不再接受 `providerBinaryPath`；provider executable 只来自 daemon 启动配置，embedded GUI 在启动 daemon 时解析本地 tools/system binary。
- updater 强制 SHA-256、响应大小和超时；version 必须是合法 semver，asset name 必须是单一 basename。native bundle 不在应用目录内自替换二进制。

### 3.3 Route access scope

将 access 配置从：

```text
route_id -> RouteAccessConfig
```

迁移为：

```text
(tunnel_id, route_id) -> RouteAccessConfig
```

- Set/List access API 增加 tunnel scope。
- 删除 route、replace routes、删除 tunnel 级联清理 access 配置。
- 旧 state 启动时迁移；同 id 跨多个 tunnel 的歧义项不猜测，保留 legacy 状态并在 diagnostics 提示。
- route/tunnel ID 限制为安全 slug，拒绝保留 ID、控制字符和路径分隔符。
- gateway cookie 名加入稳定 tunnel scope 或 hash。

### 3.4 生命周期和持久化

- 每个 tunnel 引入 operation generation；start spawn 完成后校验 generation 和 shutdown 状态。
- stop/delete/shutdown 可以取消 in-flight start。
- monitor、gateway、SSE、WebSocket 统一纳入 task tracker/cancellation。
- state/settings 使用临时文件、flush/sync、atomic rename；Unix 文件创建时强制 0600，目录按平台收紧权限。
- daemon data directory 增加单 writer/跨进程保护，避免固定 `.json.tmp` 被并发写坏。
- 持久化失败不返回成功；内存和磁盘不一致时保留最后成功快照并报告明确错误。

### 3.5 GUI、CLI 和 updater 行为

- GUI route+gate 保存失败时补偿/回滚，并强制重新读取 daemon snapshot。
- gate 读取失败保留最后成功缓存，显示“状态不可用”，不合成 Open。
- 删除 tunnel profile 只有 daemon delete 成功后才移除本地记录；不可达时保留 pending cleanup。
- CLI 增加统一 tunnel 选择，不再把 scoped 操作全部固定为 `primary`。
- named Cloudflare readiness 不强制要求 `public_base_url`。
- 表格截断按 Unicode 字符边界处理。
- GUI updater 对无法验证、路径不安全或超出大小限制的更新直接拒绝。

## 4. 实施批次

### Batch 0：发布解阻

- 清理 release overlay 和 sidecar 文档。
- CI 加入 Node/shell/config 门禁。
- 修复安装脚本 checksum fail-open。
- 收敛 workflow 权限。

### Batch 1：认证与 updater 安全

- loopback-only token discovery。
- auth endpoint bearer gate。
- gate credential header/cookie 过滤。
- 移除 API provider executable override。
- updater hash/size/path/timeout 校验和 native bundle 保护。

### Batch 2：生命周期与一致性

- route access scope/migration/cleanup。
- per-tunnel generation/cancellation。
- gateway/monitor/SSE/WS shutdown。
- state/settings 原子写、权限和单 writer。
- GUI route/gate 补偿、unknown gate、pending profile cleanup。

### Batch 3：CLI、性能和体验

- CLI tunnel 选择、named readiness、Unicode 输出。
- bounded health-check concurrency。
- gateway route snapshot/SSE 增量读取。
- GUI live refresh、CSP 和基础可访问性。
- 文档、版本和跨平台测试门禁。

## 5. 迁移和回滚

1. PersistedState 增加 schema version。
2. 迁移前创建带时间戳的 backup；迁移解析失败时继续使用旧文件。
3. 旧 API 字段使用 `serde(default)`，基础 route/tunnel 读写保持兼容。
4. 每个 Batch 独立 commit；批次之间不共享未完成的临时行为。
5. updater 和安全校验全部 fail-closed，不自动降级到未验证模式。
6. 任何批次验证失败时回滚该批次，不继续堆叠后续改动。

## 6. 验收标准

每批完成前运行：

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

此外必须满足：

- release config 不包含 `externalBin`。
- remote + token empty 不会读取本机 token。
- auth endpoint 无 bearer 时拒绝 forwarded/桥接请求。
- gate credential 不会出现在 upstream 请求。
- 跨 tunnel 同 id 的 access gate 隔离，删除后不残留。
- start/stop/shutdown 不会产生已停止但 provider 复活的竞态。
- state/settings 具备权限和原子写保护。
- Windows updater 不再显示可安装但必然失败。
- CLI 可以选择非 primary tunnel。
- 关键安全/生命周期路径有回归测试，GUI 至少有真实 DOM/Tauri smoke。
- 文档、release workflow 和实际 daemon ownership 一致。

## 7. 风险与回滚点

- auth endpoint 改为 bearer gate 会影响旧的手写 loopback curl；文档和错误信息同步更新，并保留 access code 作为 body 解锁凭据。
- route access schema 变化需要兼容旧 state；迁移失败必须可诊断且不丢原文件。
- native bundle updater 改为手工引导会降低自动更新便利性，但优先保证签名和权限不被破坏。
- GUI route/gate 补偿逻辑不能替代后端原子性；后续若 API 复杂度允许，再升级为单 endpoint 原子写入。

## 8. 当前不做的事情

- 不修改已完成的 embedded daemon ownership 设计。
- 不重新实现 provider 生命周期模型。
- 不在本轮引入大规模前端框架或重做 UI 视觉系统。
- 不把性能 benchmark 尚未证明的优化伪装成行为修复；先完成安全和一致性闭环。
