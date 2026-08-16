# dsh-tunnelmux-remote Design

> 2026-08-16 · 状态: approved by owner · 从零实现（方案 B），参考 @linxin666/dsh-remote-web-ui v0.1.18（Apache-2.0）的设计

## 1. 目标

做一个 DSH 插件 `dsh-tunnelmux-remote`：手机远程控制 DSH Web GUI，隧道后端**复用 TunnelMux**（4765 控制 API），而不是插件内嵌 cloudflared。功能对齐 dsh-remote-web-ui：扫码配对 QR、一次性配对 token、设备会话管理、可撤销会话、移动端页面、实时状态 SSE。

## 2. 范围

- 隧道生命周期：由插件调用 TunnelMux 控制 API 起/停 cloudflared/ngrok 隧道，等待公网 URL，异常退避重启。
- 配对安全：loopback 控制面 + LAN/public 手机面 fence、per-IP accept 限流、HttpOnly cookie 门控。
- 移动端：/m 独立页面 + /m/api/* 允许清单 RPC（workspace/session 复用宿主 apiProxy）+ SSE 事件流。
- 桌面端：侧边栏 QR 入口（设置按钮旁）、状态面板、刷新/停止按钮、SSE 实时状态。

不做：不内嵌 cloudflared；不重写 DSH 会话业务（全部转发宿主 apiProxy）；不做多播/广播。

## 3. 架构

独立仓库，与 dsh-zhihu-dashboard 同构（tsdown + cordis.patch.yml + client inject）：

    dsh-tunnelmux-remote/
    ├── package.json          # name: dsh-tunnelmux-remote
    ├── cordis.patch.yml      # 单插件行: host + browser 双面
    ├── tsdown.config.ts      # lib/ + client/ + mobile/ 三入口
    ├── src/
    │   ├── pairing.ts        # 纯 TS 配对状态机 (注入 clock/randomness)
    │   ├── tunnelmux.ts      # TunnelMux 隧道适配器 (4765 API 生命周期)
    │   ├── routes.ts         # /api/pair/* 路由族 + fence + 限流
    │   ├── mobile.ts         # /m 页面 + /m/api/* RPC 允许清单
    │   ├── index.ts          # 宿主入口 (name/inject/apply/config)
    │   └── client/           # 侧边栏 QR + 状态面板 (React)
    └── README.md

数据流：手机/桌面面板 → DSH webServer（/api/pair/*、/m/*）→ 插件宿主 → TunnelMux 4765 控制 API。
依赖：@deepseek-ai/dsh-host-apiproxy（移动端 RPC）、schemastery/zod（配置与 payload 校验）。不依赖 cloudflared npm 包。

## 4. TunnelMux 隧道适配器（核心差异点）

替代 dsh-remote-web-ui 的 TunnelManager，纯 HTTP 驱动 4765 API。
**重要修正（2026-08-16 对照 api.rs/runtime.rs 实现）**：`POST /v1/tunnel/start` 是**同步等待** provider 启动并提取公网 URL 的（`runtime.rs::wait_for_provider_startup`），响应直接带 `public_base_url`，插件**不需要轮询 status 等 URL**；且 daemon 自带 `auto_restart` + `pending_restarts` + `--max-auto-restarts` 自动重启，**插件绝不自己重启**（避免与 daemon 打架），只做状态呈现。
生命周期：`stopped → starting(调用 start) → running(响应带回 public_base_url) → 观察 status`

| 事件 | 调用 |
|---|---|
| 启动 | POST /v1/tunnel/start {tunnel_id: "dsh-remote", provider, target_url, auto_restart: true}（tunnel_id 必填，插件固定用 "dsh-remote"） |
| 拿公网 URL | 直接读 start 响应 public_base_url；为 null 时兜底查一次 GET /v1/tunnel/status |
| 状态观察 | 轮询 GET /v1/tunnel/status（间隔 5s）：running/stopped/error + last_error，仅用于面板呈现 |
| 重启策略 | 交给 daemon auto_restart（true）；插件不重启 |
| 停止/清理 | POST /v1/tunnel/stop {tunnel_id: "dsh-remote"}；dispose 时确保 stop |

- 每次轮询带 /v1/health，暴露 daemonOk；daemon 不可达 → failed + "daemon unreachable"，不重启。
- 依赖注入：factory（HTTP 客户端）、timers、pollMs 可注入，可无网络单测。

## 5. 配对与安全

- 配对服务：纯 TS 状态机。一次一个活动 token；issue() 刷新即作废旧 QR；accept() 一次性消费（复用→'used'）；token 过期拒绝；stop() 撤销全部会话并清 token。
- 设备会话：cookie dsh_pair（HttpOnly + SameSite=Lax），touchDevice/heartbeat 刷新 presence，offlineAfterMs 超时离线，10s sweep，maxDevices 默认 4（超限逐出最旧）。
- 围栏：issue/stop/events 仅 loopback；accept/heartbeat/status 允许 loopback + LAN IP + 公网 host；accept per-IP 限流 10 次/30s。
- QR：链接 = {public_url 或 LAN base}/?pair={token}；/api/pair/events SSE 推桌面面板。

## 6. 移动端

- /m 独立页面（HTML shell + lib/mobile.js + apple-touch-icon）。
- /m/api/* 允许清单：workspace.list、session.create/list/history/search/prompt/models/selectModel/rename（apiProxy 转发）+ mobile.preferences（本地应答）。其余 403。
- /m/api/events.mux SSE 桥接宿主 mux（15s 心跳，活动即 presence）。
- UI：会话列表（20/页游标分页）、新建、历史、消息流、模型选择、发送框（回车发送可配）。

## 7. 配置（settings 命名空间 tunnelmux-remote）

| 键 | 默认 | 说明 |
|---|---|---|
| enabled | true | 总开关 |
| tunnelmuxBaseUrl | http://127.0.0.1:4765 | TunnelMux 控制 API |
| tunnelmuxApiToken | (空) | 可选 Bearer（secret） |
| targetUrl | http://127.0.0.1:3080 | 暴露的本地 DSH GUI |
| tunnelProvider | cloudflared | cloudflared/ngrok |
| autoTunnel | false | 启动即自动开隧道 |
| publicBaseUrl | (空) | 已有公网入口手动指定 |
| tokenTtlMs / offlineAfterMs / maxDevices | 10min / 25s / 4 | 配对调参 |

## 8. 错误处理

- daemon 不可达 → daemon unreachable + 引导（确认 tunnelmuxd），不重试。
- start 失败/超时 → failed + 退避重启（仅运行中意外退出）。
- 配对错误码统一 {ok:false, code}：forbidden / bad-payload / lan-required / unknown-address / invalid / used / rate-limited / unpaired。

## 9. 测试

- pairing.ts 单测：token 一次性/过期/stop 撤销/presence 离线/maxDevices 逐出。
- tunnelmux.ts：注入假 HTTP，测 start→running、URL 超时、意外退出退避、daemon 不可达、stop。
- routes：假 webServer，测 fence/限流/payload schema。
- mobile：允许清单拒绝 + session.list 分页游标。
