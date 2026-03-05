# SoloMesh 接入 TunnelMux

## 1. 关系边界

- TunnelMux: 独立隧道服务
- SoloMesh: TunnelMux 客户端

SoloMesh 不再持有 provider 进程，改为调用 TunnelMux API。

## 2. 建议接入方式

1. SoloMesh 设置页新增：
- `TUNNELMUX_BASE_URL`（默认 `http://127.0.0.1:4765`）
- `TUNNELMUX_API_TOKEN`（可选）

2. 远程访问按钮行为：
- 调 `GET /v1/tunnel/status`
- 若未运行则调 `POST /v1/tunnel/start`
- 再按业务目标调 `POST /v1/links/issue`（或读取 public base）

3. 路由策略：
- 使用 host-based 优先
- path-based 仅用于无法分配子域时

## 3. 迁移顺序

1. 并行运行现有 TS 内核与 TunnelMux
2. SoloMesh 增加“外部隧道服务模式”开关
3. 功能验证通过后切换默认
4. 最后移除内置隧道实现
