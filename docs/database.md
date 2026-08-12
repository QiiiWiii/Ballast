# 数据库边界

PostgreSQL 保存可恢复的业务事实：

- 执行任务及其版本；
- 系统共享策略模板及不可变版本；
- 交易所健康状态迁移和真实行情订阅状态；
- 子订单意图和交易所订单标识；
- 去重后的成交；
- 任务事件；
- 待发送命令 outbox。

数据库不默认保存全部实时盘口流。执行决策需要的行情摘要应随事件保存；明确请求回补的 OHLCV/逐笔成交进入历史表；股票研究行情按交易日保存为本地 `JSON.zst` 文件，PostgreSQL 只保存 research manifest、SHA-256、路径、任务状态和验证结果。持续 L2 需要单独启用采集与容量方案。

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

## 策略研究

迁移 `0007_strategy_research_lab.sql` 增加：

- 通用资产分类和数据来源字段，以及不绑定执行场所的 TSLA research instrument；
- `research` 策略 scope 与强类型 `algorithm_config`；
- Alpaca/IEX 下载任务和逐日 manifest；
- 固定 TSLA 验证案例、不可变运行快照、逐日候选结果和 append-only 决策。

Alpaca 凭证只存在于进程环境。下载记录、错误码和 report 均不得包含 key 或上游响应头。
