# 系统架构

## 目标

Ballast 统一管理多交易所执行任务，并逐步支持 TWAP、POV、真实订单生命周期以及双腿对冲。系统优先保证状态可恢复、数值精确和故障可解释，而不是追求微秒级延迟。

## 组件

```text
Operator / Web
      |
      | REST + WebSocket
      v
ballast-server
      |
      v
Rust execution engine ------> PostgreSQL
  | strategy / risk              task, order, fill,
  | state / hedging              event, outbox
  |
  | typed gRPC
  v
Node exchange gateway
  | ccxt + exchange-specific normalization
  v
Binance / OKX / Bybit / Gate.io / Bitget
```

## 不可破坏的边界

1. Rust 是任务、订单、成交、风险和敞口的唯一权威状态。
2. Node 网关不得保存策略进度，也不得自行决定业务级重试。
3. `proto/exchange_gateway.proto` 是跨语言唯一契约。
4. 价格和数量跨边界使用十进制字符串，禁止 JavaScript `number`。
5. 外部系统只保证至少一次事件；Rust 必须按稳定事件 ID 去重。
6. 下单超时不是下单失败，而是 `submission_unknown`，必须查询和对账。
7. 行情过期、能力不支持或账户状态不确定时显式停止相关执行。

## 运行时隔离

早期可以由一个 Node 进程承载多家交易所适配器。进入私有订单流和真实下单阶段前，部署应支持按交易所或账户组拆分网关实例，降低单点故障范围。

## 当前骨架范围

当前仅提供：

- 精确领域类型与最小 TWAP/POV 计算；
- 执行状态机和模拟成交的最小边界；
- gRPC 接口定义及健康检查；
- PostgreSQL 初始表；
- axum 健康检查；
- 本地 Compose 和 CI。

除健康检查外，交易所 RPC 目前明确返回 `UNIMPLEMENTED`。
