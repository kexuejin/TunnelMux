# dsh-tunnelmux-remote Implementation Plan

> 2026-08-16 · 前置设计: docs/plans/2026-08-16-dsh-tunnelmux-remote-design.md (commit bd6f928)

## 阶段 0：仓库脚手架
- [ ] 在 ~/source 下创建独立仓库 dsh-tunnelmux-remote（git init，参考 dsh-zhihu-dashboard 结构）。
- [ ] package.json：name dsh-tunnelmux-remote，dsh.bundle.patch + dsh.client.inject（runtime/connection/ui-settings/ui-sidebar），peerDeps @deepseek-ai/cordis + dsh-tools。
- [ ] cordis.patch.yml 单插件行（host + browser 双面）。
- [ ] tsdown.config.ts：lib/index、client、mobile 三入口；vitest 单测配置。
- [ ] 配置文件模板 + 依赖安装（@deepseek-ai/dsh-host-apiproxy、schemastery、zod）。

## 阶段 1：配对状态机（纯 TS，先于一切）
- [ ] src/pairing.ts：token 生命周期（issue 刷新作废、accept 一次性消费、过期拒绝、stop 撤销）、设备会话表（maxDevices 逐出最旧、presence touch/heartbeat/sweep）、snapshot 派生（lan-required/waiting/disconnected/connected/stopped）、onState 通知（dedupe）。
- [ ] 注入 clock/randomness；单测覆盖：token 一次性/过期/stop/presence 离线/maxDevices 逐出/issue 无 base 抛错。

## 阶段 2：TunnelMux 隧道适配器
- [ ] src/tunnelmux.ts：HTTP 客户端（fetch 4765，可选 Bearer token），start/status/stop/health 封装。
- [ ] TunnelMuxTunnelManager：stopped→starting→running→failed 状态机；轮询 status 等 public_base_url（30s 超时/1s 间隔）；意外退出退避重启（5s→60s 指数，仅运行中）；daemon 不可达 → failed 不重启；dispose 停止。
- [ ] 依赖注入 factory/timers/pollMs；单测（假 HTTP）：start→running、URL 超时、意外退出退避、daemon 不可达、stop。

## 阶段 3：宿主路由与围栏
- [ ] src/routes.ts：/api/pair/issue|accept|stop|heartbeat|status|events 路由族；loopback fence + LAN/public fence；accept per-IP 限流（10 次/30s）；payload schema 校验；SSE fan-out。
- [ ] LAN 地址推导（非内网 IPv4，interface 顺序）。
- [ ] 路由单测（假 webServer）：fence/限流/bad-payload/错误码。

## 阶段 4：移动端
- [ ] src/mobile.ts：/m 页面 + /m/mobile.js + apple-touch-icon 静态路由；/m/api/* 允许清单 RPC（workspace.list、session.* 经 apiProxy 转发，mobile.preferences 本地应答）；/m/api/events.mux SSE 桥接（15s 心跳）。
- [ ] mobile 入口 bundle：会话列表（20/页游标分页）、新建、历史、消息流、模型选择、发送框；配对 cookie 门控。
- [ ] 允许清单拒绝测试 + session.list 分页游标单测。

## 阶段 5：桌面客户端（侧边栏）
- [ ] src/client/：设置按钮旁 QR 入口（qrcode.react）、状态面板（daemon/tunnel/配对阶段/设备列表）、刷新 issue、停止 stop、SSE 状态。

## 阶段 6：配置与装配
- [ ] settings 命名空间 tunnelmux-remote（§7 配置表，tunnelmuxApiToken 标 secret）。
- [ ] src/index.ts：name/inject（webServer、apiProxy）/apply 装配全部模块；autoTunnel 开关接适配器。
- [ ] 构建产物检查（lib/client/mobile 三 bundle）与 typecheck。

## 阶段 7：端到端验证
- [ ] 本机起 tunnelmuxd（4765 可达）→ 插件加载 → issue QR → accept 配对 → 手机页面会话操作。
- [ ] 手机经公网 URL 访问（cloudflared/ngrok 隧道）验证 fence（public host 放行）。
- [ ] README：安装、配置、安全说明、与 dsh-remote-web-ui 的关系。

## 验收标准
- 手机扫码 → 配对 → 会话列表/新建/发消息全链路可用；
- 隧道由 TunnelMux 管理（插件内无 cloudflared 依赖）；
- 全部单测通过（pairing/tunnelmux/routes/mobile）；
- 未配对设备访问 /m/api/* 一律 403；stop 后会话立即失效。
