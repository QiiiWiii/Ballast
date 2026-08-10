# 历史成交策略回放

Ballast 可以在 PostgreSQL 已保存的真实逐笔成交上运行可重复的 TWAP/POV 回放。首版执行模型固定为 `trade_vwap_proxy/v1`：每个策略切片使用窗口内逐笔成交的数量加权市场 VWAP，并按方向应用显式额外滑点和手续费。它不是 L2 订单簿重建，不模拟排队位置、挂单成交概率或真实市场冲击。

## 确定性与数据边界

- 输入绑定不可变的策略模板版本，并保存完整参数快照、执行模型版本和成本假设。
- 执行前必须存在覆盖完整请求区间的 `completed` 逐笔成交补数任务。没有补数、补数失败/未完成或时间范围覆盖不足分别返回稳定的 `409` 错误，不会生成高可信度结果。
- 每条历史成交有数据库分配的单调 `ingestion_id`。回放在只读 `REPEATABLE READ` 事务中冻结 PostgreSQL snapshot 和当前区间的最大 `ingestion_id`，之后按 `trade_time + exchange_trade_id` keyset 分页且只读取该上界以内的数据；并发补数不会让分页跳过或混入成交，未提交事务也不会在后续页面突然出现。
- 运行保存数据快照谓词（PostgreSQL snapshot、隔离级别、标的、区间、最大 `ingestion_id`、排序、数量和首尾键）、参与覆盖判定的补数任务快照，以及当次计算使用的完整 instrument 规格快照。后续同步修改合约类型、合约面值、数量步长、资产或费率时，不会改写旧运行的计算依据。
- 历史成交数量沿用网关的 native quantity 语义，并通过 `Instrument` 的实时执行转换规则处理 base、quote 和 contracts。
- 空窗口不会合成成交；无数据运行保存为 `failed / historical_trades_not_found`。
- 首尾覆盖不足、超过阈值的内部时间缺口、空窗口或残余量会使运行成为 `completed_with_warnings`，可信度为 `limited`。
- 单次运行最多读取 1,000,000 条成交并生成 10,000 个切片，超过边界会明确拒绝。

TWAP 使用 `ballast-strategies::Twap` 按剩余时间切片；POV 使用 `ballast-strategies::Pov` 根据窗口内真实成交量和模板参与率生成切片。代理模型最多使用窗口内真实成交量，不跨窗口借用流动性。

市场与执行 VWAP 都按逐笔/逐切片的 base、quote 数量聚合为 `sum(quote) / sum(base)`。这对 spot、linear 和 inverse 使用同一公式；inverse 的 base 数量会按每个价格以 `contracts * contract_size / price` 换算，不能把全部 contracts 在最终均价上一次性换算。`fee_amount`、`explicit_slippage_amount` 和 `implementation_shortfall_amount` 均以 quote 等值表示。

## API

- `POST /api/v1/replays`：同步计算并原子保存运行、切片与指标。
- `GET /api/v1/replays/{run_id}`：运行输入、状态、覆盖、缺口和限制。
- `GET /api/v1/replays/{run_id}/slices`：稳定排序的切片决策和模拟结果。
- `GET /api/v1/replays/{run_id}/metrics`：VWAP、implementation shortfall、成交率、参与率、费用等指标。

创建请求必须使用策略模板版本相同的 `quantity_unit`，历史区间长度必须等于模板的 `duration_seconds`。时间戳最大精度为 PostgreSQL 可无损保存的微秒；更细精度返回 `422 timestamp_precision_invalid`。重复 `idempotency_key` 且请求一致时返回原运行；键相同但请求不同返回 `409 replay_idempotency_key_conflict`。

覆盖错误语义：

- `replay_history_backfill_required`：请求区间没有逐笔成交补数任务；
- `replay_history_backfill_incomplete`：相关补数仍为 `pending/running`；
- `replay_history_backfill_failed`：相关补数失败；
- `replay_history_coverage_insufficient`：已有完成任务但不能连续覆盖整个区间。

## OKX BTC/USDT 示例

先同步标的并补齐一分钟真实成交。假设已经创建一个 60 秒、10 秒切片的 `base_quantity` TWAP 版本 `TWAP_VERSION_ID`，以及相同周期、参与率 10% 的 POV 版本 `POV_VERSION_ID`。

```bash
curl -X POST http://localhost:8080/api/v1/replays \
  -H 'content-type: application/json' \
  -d '{
    "exchange":"okx",
    "market_kind":"spot",
    "symbol":"BTC/USDT",
    "template_version_id":"TWAP_VERSION_ID",
    "side":"buy",
    "requested_amount":"0.01",
    "quantity_unit":"base_quantity",
    "start_at":"2026-08-01T00:00:00Z",
    "end_at":"2026-08-01T00:01:00Z",
    "idempotency_key":"okx-btc-twap-20260801-0000",
    "fee_rate":"0.001",
    "extra_slippage_bps":"2",
    "gap_threshold_seconds":20
  }'

curl -X POST http://localhost:8080/api/v1/replays \
  -H 'content-type: application/json' \
  -d '{
    "exchange":"okx",
    "market_kind":"spot",
    "symbol":"BTC/USDT",
    "template_version_id":"POV_VERSION_ID",
    "side":"buy",
    "requested_amount":"0.01",
    "quantity_unit":"base_quantity",
    "start_at":"2026-08-01T00:00:00Z",
    "end_at":"2026-08-01T00:01:00Z",
    "idempotency_key":"okx-btc-pov-20260801-0000",
    "fee_rate":"0.001",
    "extra_slippage_bps":"2",
    "gap_threshold_seconds":20
  }'
```

查询两个 `run_id` 的 `/metrics`，可直接比较 `simulated_execution_vwap`、`implementation_shortfall_bps`、`fill_rate`、`actual_participation_rate`、`residual_amount` 和 `fee_amount`。比较成立的前提是两次运行使用相同标的、时间范围和成本假设。
