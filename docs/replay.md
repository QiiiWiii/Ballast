# 历史成交策略回放

Ballast 可以在 PostgreSQL 已保存的真实逐笔成交上运行可重复的 TWAP/POV 回放。首版执行模型固定为 `trade_vwap_proxy/v1`：每个策略切片使用窗口内逐笔成交的数量加权市场 VWAP，并按方向应用显式额外滑点和手续费。它不是 L2 订单簿重建，不模拟排队位置、挂单成交概率或真实市场冲击。

## 确定性与数据边界

- 输入绑定不可变的策略模板版本，并保存完整参数快照、执行模型版本和成本假设。
- 历史成交按 `trade_time + exchange_trade_id` 使用 keyset 分页和稳定顺序读取。
- 历史成交数量沿用网关的 native quantity 语义，并通过 `Instrument` 的实时执行转换规则处理 base、quote 和 contracts。
- 空窗口不会合成成交；无数据运行保存为 `failed / historical_trades_not_found`。
- 首尾覆盖不足、超过阈值的内部时间缺口、空窗口或残余量会使运行成为 `completed_with_warnings`，可信度为 `limited`。
- 单次运行最多读取 1,000,000 条成交并生成 10,000 个切片，超过边界会明确拒绝。

TWAP 使用 `ballast-strategies::Twap` 按剩余时间切片；POV 使用 `ballast-strategies::Pov` 根据窗口内真实成交量和模板参与率生成切片。代理模型最多使用窗口内真实成交量，不跨窗口借用流动性。

## API

- `POST /api/v1/replays`：同步计算并原子保存运行、切片与指标。
- `GET /api/v1/replays/{run_id}`：运行输入、状态、覆盖、缺口和限制。
- `GET /api/v1/replays/{run_id}/slices`：稳定排序的切片决策和模拟结果。
- `GET /api/v1/replays/{run_id}/metrics`：VWAP、implementation shortfall、成交率、参与率、费用等指标。

创建请求必须使用策略模板版本相同的 `quantity_unit`，历史区间长度必须等于模板的 `duration_seconds`。重复 `idempotency_key` 且请求一致时返回原运行；键相同但请求不同返回 `409 replay_idempotency_key_conflict`。

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
