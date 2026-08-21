# TunnelMux 网关按路由访问控制设计（access-code 门禁）

> 2026-08-21 · approved by owner (方向确认) · 门禁页 + cookie 交互

## 1. 背景

控制面 4765 已有 token + 认证码解锁(require 模式,见 2026-08-21-control-auth-design)。
但网关 data-plane(48081) 是纯反向代理,公网经 cloudflared→48081→各 upstream,网关本身无鉴权。
不同服务暴露面不同,需要按路由粒度的访问控制。owner 拍板:网关不同服务配置不一样。

## 2. 目标

- 每路由可配置是否需要访问凭证(access-code / token);deepseek 等公开路由不开,
  codex/rmux 等敏感服务开启。
- 交互: 访问受保护路由且无凭证 → 返回门禁页(输码表单);输对种 cookie 放行至窗口过期。
- GUI/CLI 均可配置。

## 3. 机制(owner 确认)

- 每路由 access-code / token(RouteRule 粒度的凭证)。
- 门禁页 + cookie(同源同站点,HttpOnly+SameSite;窗口过期自动回门禁)。

## 4. 架构与组件

- **core RouteRule** 加字段(仿 forward_host_header 的 #[serde(default)]):
  - require_access_code: Option<String> —— 路由的访问码(空/None=公开)。
  - access_cookie_ttl_ms: Option<u64> —— 本路由门禁 cookie 有效时长(默认取自 daemon 全局)。
- **gateway.rs proxy_request_for_tunnel**: 匹配路由后、转发 upstream 前,若该路由
  require_access_code 有值且请求未通过,则:
  - 校验: 请求 cookie 或 Authorization: Bearer 是否等于该路由 access_code。
  - 未通过 → 返回门禁页(HTML 表单,POST 提交码)或 401(对非浏览器/API 请求)。
  - 通过 → Set-Cookie(HttpOnly+SameSite=Strict, Max-Age=ttl) 并放行。
- **COOKIE 名**: 形如 tunnelmux_access_<route_id>(同源隔离,避免路由间串 cookie)。

## 5. 数据流

    公网 → cloudflared → 48081 → proxy_request_for_tunnel
          → (门禁检查: 路由需要码? 已带对 cookie/token?)
              ├─ 未通过 → 门禁页/401
              └─ 通过 → 转发 upstream

## 6. 配套面

- CLI: route add/edit 加 --require-access-code <code>;route list 显示是否受保护。
- control-client: CreateRouteRequest / RouteRule 透传新字段。
- GUI: 服务(路由)编辑区加字段(是否要凭证 + 码),provider 无关。

## 7. 错误处理

- 未带凭证 → 200 门禁页(浏览器) / 401(API,Bearer 缺)。
- 错码 → 首页 401 + 门禁页重渲染带错误提示。
- cookie 过期/缺失 → 重新门禁。
- WebSocket 升级请求 → 若路由受保护,门禁检查同样适用(Upgrade 非浏览器选择器,直接 401)。

## 8. 与现有机制关系

- 控制面 4765 鉴权不变。网关门禁是 data-plane 的独立一层。
- 对已带 Authorization 的上游(如 DSH 的 trustedHosts fence),门禁 cookie 与其共存不冲突。

## 9. 测试

- gateway 单测(假 route/req): 受保护路由无凭证→门禁页;带对 cookie→放行;带对 Bearer→放行;
  错码→拒绝;公开路由无门禁。
- core: RouteRule 序列化(字段缺省 false/None 向后兼容)。
- CLI/control-client/gui: 参数透传 + 往返。
