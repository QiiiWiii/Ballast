# 数据库边界

PostgreSQL 保存可恢复的业务事实：

- 执行任务及其版本；
- 系统共享策略模板及不可变版本；
- 交易所健康状态迁移和真实行情订阅状态；
- 子订单意图和交易所订单标识；
- 去重后的成交；
- 任务事件；
- 待发送命令 outbox。

数据库不默认保存全部实时盘口流。执行决策需要的行情摘要应随事件保存；明确请求回补的 OHLCV/逐笔成交进入下述历史表，持续 L2 需要单独启用采集与容量方案。

Node 网关不维护权威业务数据库。网关重启后由 Rust 使用订单、成交和持仓查询进行对账。

`0003_strategy_templates_and_ui_analytics.sql` 将迁移前任务的参数组合转换为明确归档的模板版本，并让所有任务强制引用模板版本。新代码不接受旧的直接策略参数创建契约。

`0004_live_execution_risk_and_hedging.sql` 只新增 live 所需事实：

- managed/native 模板后端和交易所专属算法快照；
- 多账户元数据与 secret 引用（不保存密钥）；
- 账户快照、对账批次和逐项差异；
- 普通订单、原生算法父/子订单和两类订单共用的去重成交；
- 审批、零默认风险限额、kill switch 和风险决策审计；
- 对冲配置、运行、敞口批次和人工介入；
- 单次、短时 WebSocket ticket 的哈希与消费状态。

live 任务从 `pending_approval` 开始；创建者与批准者必须不同。审批只改变调度资格，不替代提交前的风险检查。

`0005_exchange_health_current_latency.sql` 在 `exchange_health` 快照中保存最近一次实际探测耗时。状态迁移历史继续只记录状态或错误码变化，运营页面不再从历史事件推断当前延迟。
## 历史行情表

迁移 `0006_historical_market_data.sql` 增加：

- `historical_backfill_jobs`：请求参数、状态、持久化游标、失败码和真实覆盖范围。
- `historical_candles`：按标的、周期和开盘时间幂等保存 OHLCV。
- `historical_trades`：按标的和交易所原始成交 ID 幂等保存逐笔成交。

任务页数据与游标在同一个事务内提交。重复执行不会增加行情表记录；中断后从已提交的 `cursor_at` 继续。

迁移 `0007_trade_replay.sql` 增加：

- `replay_runs`：不可变输入、策略快照、模型版本、数据覆盖、缺口与可信度。
- `replay_slices`：逐窗口策略决策、真实成交量、市场 VWAP 和代理执行结果。
- `replay_metrics`：VWAP、implementation shortfall、成交率、参与率、费用与显式滑点。

三类回放记录在同一个事务中写入；`idempotency_key` 唯一，重复请求不会生成第二组切片或指标。

迁移 `0008_replay_data_snapshot.sql` 为逐笔成交增加数据库分配的单调 `ingestion_id`，并让回放运行保存 `data_snapshot` 与 `coverage_snapshot`。回放分页在只读 `REPEATABLE READ` 事务中使用已冻结的 PostgreSQL snapshot 与最大 `ingestion_id`，所以补数在分页期间插入更早排序键的成交也不会改变本次数据集；快照同时记录完整覆盖所依赖的补数任务。旧的 `0007` 运行会被明确标记为 `unverifiable_pre_0008`，不会伪装成可复核快照。
