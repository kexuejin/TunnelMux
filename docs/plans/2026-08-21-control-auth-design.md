# TunnelMux 控制面鉴权设计（默认强制 + 三档开关）

> 2026-08-21 · 状态: approved by owner · 方案 A：强制静态 Bearer Token

## 1. 背景与暴露面

- 控制面 API（默认 `127.0.0.1:4765`）当前**未配置 token 即全开放**（`--api-token`/`TUNNELMUX_API_TOKEN` 可选，缺省返回 true 放行）。
- 鉴权中间件 `control_auth_middleware` 已实现（Bearer + `GET /v1/health` 豁免），只是默认未启用。
- 本地工具链（`tunnelmux-cli` `--token`、`tunnelmux-gui` settings.token、`control-client`、`dsh-tunnelmux-remote` 插件 tunnelmuxApiToken）已支持传 token。
- 风险:①本机其他进程/恶意软件可完全控制隧道/路由/设置;②一旦将 4765 桥接外网(SSH/网关转发)即完全裸奔。

## 2. 目标

- **默认 fail-closed 强制鉴权**,但仍保留**可显式降级的开关**,避免一刀切破坏已有部署。
- 本地 CLI/GUI/插件**自动认领**生成的 token,现有工具体验不断裂。
- 明确迁移路径与哪些场景需降级。

## 3. 三档安全模式

`--control-auth <mode>` + `TUNNELMUX_CONTROL_AUTH` 环境变量:

| mode | 含义 | 适用 |
|---|---|---|
| `require`（默认） | 未配 token 也 fail-closed(拒绝 + 自动生成 token 落盘) | 新部署、重视安全 |
| `optional` | 配了 token 就校验,没配就开放(向后兼容) | 已有免 token 部署、迁移期 |
| `off` | 永不校验 | 仅本地开发调试 |

简化结论(owner 拍板):默认 `require`,`optional`/`off` 作为逃生通道显式降级;不为特定服务保留 optional——只是要这个能力存在。

## 4. Token 生命周期

- 优先级: `--api-token` > `TUNNELMUX_API_TOKEN` > 自动生成。
- 自动生成:仅 `require` 模式且未显式配置时,首启生成 32 字节随机 hex,写入 `~/.tunnelmux/api-token`(0600),daemon 启动日志提示路径。
- **显式 token 不自动写盘**(避免敏感物落盘);只有自动生成的才写盘(本地工具需读取)。
- 本地工具自动认领: CLI/GUI/control-client/dsh-tunnelmux-remote 读取 `~/.tunnelmux/api-token`(若存在)自动带 Bearer。

## 5. 错误处理

- 未认证访问受保护端点 → `401 {"error":"unauthorized: missing or invalid bearer token"}`(health 豁免)。
- `require` + 无 token 启动: 自动生成并打印 "API token written to ~/.tunnelmux/api-token"。
- `optional` + 无 token: 放行(现状),日志提示 "control API open (no token, optional mode)"。

## 6. 迁移指引

- 告知 `require` 会让手写 curl/第三方脚本 401。
- 降级命令: `TUNNELMUX_CONTROL_AUTH=optional` 或配置固定 `--api-token`。
- 生产 managed-daemon 启动脚本改: 传 `--control-auth require`(或依赖 env),并确保客户端读得到同一 token 文件。

## 7. 测试

- 三模式: require 无 token 401/有 token 200/错 token 401;optional 无 token 200 有 token 校验;off 永 200。
- health 豁免: 任意模式 `/v1/health` 200。
- token 自动生成落盘(0600) + CLI/control-client 自动读取路径单测。

## 8. 范围

- 控制面 API 鉴权(4765)。不涉及网关 data-plane 访问控制(48081 反向代理,靠 upstream 自身防护;DSH 本体靠 trustedHosts fence)——后续另行规划。