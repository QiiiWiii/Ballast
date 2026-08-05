# AGENTS.md

本文件约束在 Ballast 仓库内工作的开发者与自动化 agent。

## 工程原则

- 默认遵循“不兼容、不兜底、不冗余”：语义变化时收敛旧逻辑，不用静默 fallback 掩盖错误，不保留重复或低价值实现。
- Rust 是业务事实源，负责策略、任务状态、风险、持久化和对冲编排。
- Node/ccxt 网关只负责交易所通信、规范化、连接、限频和安全的传输级重试。
- `proto/` 是 Rust 与 Node 之间唯一正式契约。禁止暴露 ccxt 对象或无类型的通用方法调用。
- 价格、数量、金额和手续费禁止使用二进制浮点数。跨语言协议使用十进制字符串，Rust 内部使用 `rust_decimal::Decimal`。
- 外部创建订单调用不允许盲目重试。超时必须进入不确定状态，并通过稳定的 `client_order_id` 查询和对账。
- 不支持的交易所能力必须显式失败，禁止静默改用另一种订单类型或数据来源。

## 目录所有权

- `crates/ballast-core`：无基础设施依赖的领域类型。
- `crates/ballast-strategies`：纯策略计算，不执行网络或数据库 I/O。
- `crates/ballast-execution`：执行状态机、调度和命令边界。
- `crates/ballast-simulator`：确定性成交估算。
- `crates/ballast-gateway-client`：由 Protobuf 生成的客户端和薄封装。
- `crates/ballast-storage`：PostgreSQL repository 和迁移入口。
- `crates/ballast-server`：HTTP/WS 边界与依赖装配，不放业务规则。
- `gateway-node`：交易所适配，不保存权威任务状态。

## 修改规则

- 修改 Protobuf 时，同时更新 Rust 和 Node 构建检查；破坏性变化提升 package 版本，不保留模糊兼容分支。
- 修改任务、订单或成交状态时，必须补状态迁移和重复事件测试。
- 修改数据库结构时只新增迁移，不重写已经发布的迁移。
- 不提交真实密钥、`.env`、数据库文件、构建产物或 `node_modules`。
- 未明确授权时不得调用真实下单接口。

## 完成标准

提交前至少运行：

```bash
make check
```

涉及部署配置时额外运行：

```bash
docker compose -f deploy/compose.yaml config
```

涉及数据库迁移、交易所协议或真实订单状态时，需要相应的集成测试或清晰说明尚未验证的边界。
