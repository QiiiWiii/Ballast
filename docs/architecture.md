# 系统架构

## 当前可运行链路

```text
Browser
  │ REST / WebSocket
  ▼
nginx ──► ballast-server (Rust)
             │ task state / paper IOC / event sequence
             ├────────────► PostgreSQL 17
             │ typed gRPC
             ▼
       gateway-node (ccxt 4.5.58 + ccxt.pro)
             │
             ├─ Binance
             ├─ OKX
             ├─ Bybit
             ├─ Gate.io
             └─ Bitget
```

Rust 是任务、策略、精度转换、模拟成交、持久化和事件的事实源。Node 网关只负责交易所通信、限频、字段标准化、连接状态和有边界的传输恢复。私有服务协议已经定义，但运行时默认关闭。

`ballast-execution` 承载执行状态机、`Clock`、市场/账户/执行端口和 managed 切片决策。`ballast-server` 只负责领取任务、装配网关/数据库端口并持久化领域结果。事件与 outbox 继续使用 PostgreSQL 事务，不引入额外消息中间件。

## 公共行情契约

`MarketDataService` 只暴露已经实现的 `Health`、`ListInstruments`、`GetCapabilities`、`GetOrderBook`、`WatchOrderBook` 和 `WatchTrades`。行情流通过 `connecting/connected/reconnecting/stale/closed` 状态信封报告缺口。协议中的价格、数量、费率和合约面值均为十进制字符串。

## 纸面执行

任务绑定单一交易所、单一标的和一个不可变策略模板版本。数量单位必须是 `base_quantity`、`quote_notional` 或 `contracts`。模板归档后不能创建新任务，但历史任务仍保留完整参数快照。worker 从 PostgreSQL 使用 `FOR UPDATE SKIP LOCKED` 领取到期任务：

1. 截止时间已过则进入 `expired`。
2. 获取最新盘口并检查网关接收时间；失败或过期时显式 `paused`。
3. TWAP 按当前剩余时间重新均摊，不补发错过的 tick。
4. POV 使用共享逐笔订阅中本任务时间窗口内的成交量；重启后不会重放停机成交。
5. 将任务单位转换为交易所原生数量，按 step size 向下取整。
6. 使用价格保护范围内的盘口深度模拟 IOC；未成交量回到任务残余。
7. 在同一事务中写入切片、任务进度和有序事件。

不足一张合约、最小数量或最小名义金额的残余会被保留，任务可完成但不会伪造精确成交。

## HTTP 与实时事件

Rust 的 `/api/v1` 提供交易所健康、真实订阅状态、标的同步、版本化策略模板、任务创建/查询/取消、执行分析、切片和事件。创建任务必须携带 `Idempotency-Key`。错误响应是稳定的 `error.code + error.params`。WebSocket 按事件序号支持断线补发。

## 前端

React 工作台按交易生命周期组织为控制舱、执行任务、策略中心、执行分析、市场与通道、系统状态和后续能力。交易所是筛选与诊断维度，不拆成五个重复主页面。纸面阶段的零轴图明确标记为“执行残余”；前端尚未接入账户快照，因此不得显示“净敞口”。

## 部署与观测

Compose 只向宿主机暴露 nginx Web/API 入口。PostgreSQL 和 gRPC 只在 Compose 网络内可达。Prometheus 抓取 Rust `/metrics`，备份服务定时生成 PostgreSQL custom-format dump 并按保留天数清理。

## 私有协议与安全锁

`private_gateway.proto` 定义独立的 `AccountService`、`TradingService` 和 `AlgorithmicTradingService`。普通订单包含 `submission_unknown`；原生算法使用 Binance/OKX 强类型 `oneof`，不允许透传 ccxt 对象或任意方法名。

默认网关不会读取交易所 API Key。启用 `deploy/compose.private.yaml` 后，网关仅从挂载 secret 文件装配 OKX 只读账户，并开放余额、线性永续仓位和普通未结订单的 `AccountService.GetAccountSnapshot`；所有金融数值都保持为十进制字符串，线性永续仓位按 `contracts × contractSize` 归一为有符号基础资产数量，反向永续因数量单位未进入协议而显式失败，缺失稳定 `client_order_id` 的订单同样失败。条件单或算法订单无法由普通订单契约准确表达，只要账户中存在此类未结订单，整个快照就显式失败。`TradingService`、私有流和原生算法提交继续返回 `FAILED_PRECONDITION/private_services_disabled`。Rust 在 `BALLAST_LIVE_ENABLED=true` 时仍拒绝启动，直到只读账户对账和提交前风控真正完成。

以下能力仍需后续安全实现：

- OKX 之外的只读账户快照与私有流适配器
- 价格保护 IOC、稳定 `client_order_id` 和 `submission_unknown` 对账适配器
- 限额默认零的风控层
- 成交驱动双腿对冲和裸露时间熔断

真实下单不得在这些边界完成前启用。

优先级、验收门槛与非目标见 [路线图](roadmap.md)。
