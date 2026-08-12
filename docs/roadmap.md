# 路线图

本文描述 Ballast 相对当前代码的真实差距与验收门槛。状态以仓库实现为准，不以计划书替代。

相关文档：[架构](architecture.md)、[部署](deployment.md)、[交易所能力](exchange-capabilities.md)、[ADR 0003 审批与零默认风控](adr/0003-live-approval-and-zero-default-risk.md)。

## 当前基线（paper EMS）

已可运行并作为公开默认路径：

- 五家公共行情：标的同步、REST/WS 盘口与成交、健康与 degraded 显式状态
- 版本化策略模板与纸面 TWAP / POV（`managed_ioc` + 模拟 IOC）
- 执行状态机、切片与有序事件、WebSocket 断线补发
- 运营控制舱、执行分析、市场与通道、系统状态
- 可恢复公共历史回补
- Compose 预构建多架构镜像、备份与 Prometheus；生产域名前置 Caddy 可独立部署

明确未启用：

- 交易所私有账户、真实下单与原生算法提交
- OIDC / RBAC
- 生产限额、kill switch 运行时、订单对账闭环
- `BALLAST_LIVE_ENABLED=true`（服务检测到该值会拒绝启动）

粗粒度成熟度（供优先级判断，非精确度量）：

| 平面 | 约成熟度 | 说明 |
| --- | --- | --- |
| 纸面 EMS | 70–80% | 端到端可演示；仍有运营与可观测性打磨空间 |
| Live OEMS | 10–20% | 契约、迁移与锁舱页面已铺垫；安全与执行闭环未完成 |

## 非目标（当前阶段不做）

- 重新引入股票/Alpaca 或其它非加密研究实验台
- 用代理、备用域名或自动换所规避交易所地域限制
- 静默改写不支持的订单类型、算法或数据源
- 在缺少 OIDC、对账与零默认风控前开启 live
- 盲目重试外部下单；超时必须进入不确定态并按 `client_order_id` 查询
- 为“可能的未来”预留无真实需求的抽象层或多套并行路径

## 发布轨道

### v0.1 — 纸面正式版（优先）

把当前 paper 路径固化为可引用的开源基线。

验收：

- [ ] `CHANGELOG` 收敛为 `0.1.0` 说明；去掉与代码不符的表述
- [ ] `git tag v0.1.0` 触发 Docker Hub 多架构镜像（`0.1.0` / `0.1` / `v0.1.0` / `latest` / `sha-*`）
- [ ] `deploy/compose.yaml` 在 amd64 与 arm64 上均可 `pull` 后启动
- [ ] 文档路径一致：README、部署、架构、能力矩阵与安全边界对齐
- [ ] 至少一条书面 smoke：同步标的 → 模板 → 纸面 TWAP → 切片/事件可读

### v0.2+ — Live 安全门槛（P0，可启用开关的前置）

全部完成后，才允许将 `BALLAST_LIVE_ENABLED` 从“强制拒绝”改为“配置开启”。顺序固定，不并行跳步。

#### P0.1 身份与权限

- OIDC discovery / JWKS 验签；`iss` / `aud` / 时钟偏移校验
- 角色：`viewer` / `operator` / `admin`（与 ADR 0003 审批分工一致）
- HTTP Bearer；WebSocket 单次 ticket，消费后失效
- 未认证请求不得访问任务写路径与任何私有平面接口

验收：错误登录被拒；角色矩阵有自动化测试；ticket 不可重放。

#### P0.2 密钥与私有网关

- 交易所只读/交易密钥仅经 secrets 挂载，不入仓库、日志、错误体、数据库明文
- `private_gateway` 中 `AccountService` / `TradingService` 按单交易所适配启用，能力缺失显式失败
- 网关仍不拥有策略、任务权威状态或风控决策

验收：无私钥时私有 RPC 失败关闭；有密钥时账户快照与余额/仓位只读可读。

#### P0.3 对账与不确定订单

- 稳定 `client_order_id` 生成与持久化
- 下单超时 / 网络中断 → `submission_unknown`，禁止盲目重试
- 查询适配器：按 client id / exchange id 收敛到终态或可审计挂起
- 启动与周期对账：本地 open 订单 vs 交易所 open 订单差异可告警

验收：模拟超时用例进入 unknown 并完成对账；无“再发一笔相同意图却不查证”的路径。

#### P0.4 最小 live 执行（单所优先）

建议首所：文档与地域可达性较好的一家（当前公开 smoke 以 OKX 等可达所为准，以部署出口实测为准）。

- 价格保护 IOC 真实提交一条小额或沙盒单
- 再扩展 managed 纸面同构的 live TWAP 切片（仍走 Ballast 调度，非 venue native）
- 任务状态与切片、成交、手续费以十进制字符串入账

验收：创建 →（审批若已开）→ 调度 → 成交/取消/失败全链路事件；残余与最小数量规则与 paper 语义一致且无静默改写。

#### P0.5 零默认风控与 kill switch

- 全局 / 交易所 / 账户限额默认 **0**；显式配置后才可下单
- 创建、审批、提交前三次检查（见 ADR 0003）
- 三级 kill switch 任一触发即阻断新提交
- 风险决策写入审计表

验收：零限额下无法提交；打开限额后 kill switch 立即生效；审计可追溯。

#### P0.6 审批与告警

- live 任务 `pending_approval`；创建者不可自批
- 关键路径可配置（至少一种：webhook / 邮件 / 其它明确集成），覆盖 unknown 订单、对账差异、kill switch、连续提交失败

验收：自批被拒；告警在集成测试或受控演练中可触发。

**P0 总门槛：** 上述子项全部满足后，方可在受控环境将 `BALLAST_LIVE_ENABLED=true` 设为合法配置；公开文档同步修改“拒绝启动”表述。

### v0.3+ — 深度能力（P1，不阻塞首次 live 开关）

- 原生算法：Binance / OKX 生命周期验收后 `venue_native_algo` 可提交；Bybit / Gate / Bitget 在研究完成前保持 `research_required`
- 多账户、多交易所任务编排与显式失败边界
- 成交驱动双腿对冲与裸露时间熔断
- 更细的执行质量指标、交易所原生延迟契约
- 历史扩展（资金费率、持仓量、持续 L2）仅在有明确需求时做
- 内部 gRPC 认证/加密或等价网格策略（多节点部署时）

## 纸面路径可继续打磨（与 live 并行、低优先级）

不改变安全边界，可按需推进：

- worker / 盘口缓存在高压任务下的延迟与背压指标
- 控制舱与分析的空状态、降级文案与地域 degraded 说明
- 恢复演练自动化（备份 → 独立实例 → 校验任务/事件序号）
- 公网 smoke 与 CI fixture 的定期对照记录

## 运维与发布卫生

- 镜像与发版绑定：`v*` → 版本 tag + `latest`；手动 main 仅 `main` + `sha-*`
- 生产固定 `BALLAST_IMAGE_TAG` 到不可变 tag；三业务镜像同 tag
- 泄露过的 registry / 云凭据必须轮换，不在工单或聊天中粘贴 secret
- 开源仓库不包含真实 `.env`、客户数据或内部主机信息

## 决策回写

路线图条目落地后应回写：

1. `CHANGELOG.md` 的用户可见变更
2. `docs/architecture.md` / `docs/api.md` / `docs/exchange-capabilities.md` 中与行为相关的句子
3. 若改变 live 启用条件，同步 `README` 安全边界与 ADR 0003 状态说明

未完成的 P0 项不得在 UI 或文档中表现为“已可生产下单”。
