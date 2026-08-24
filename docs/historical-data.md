# 历史行情采集与回放边界

Ballast 只采集已支持交易所的公开历史行情，不需要私有 API Key，也不会合成交易所未提供的历史。首版正式支持 OHLCV 和逐笔成交；资金费率、持仓量与持续 L2 保存保留为后续扩展。

## 数据流

Rust 服务创建并持久化补数任务，Node/ccxt 网关只执行单页公共 REST 读取和规范化。每一页在同一个 PostgreSQL 事务中写入数据并推进游标，因此进程在任意页之间退出后，可以用相同 `idempotency_key` 继续。ccxt 自带交易所限频；网关对只读历史请求做最多三次指数退避重试。

- K 线幂等键：`instrument_id + timeframe + open_time`
- 成交幂等键：`instrument_id + exchange_trade_id`
- 时间统一为 UTC `TIMESTAMPTZ`，价格和数量使用 `NUMERIC(38,18)`
- `source` 保留交易所名称，成交保留原始交易所 ID
- `max_pages` 限制单次 HTTP 请求占用时间；任务未完成时返回 `pending`，再次提交同一请求继续

## 能力与回溯边界

能力由锁定的 `ccxt 4.5.58` 在运行时通过 `has.fetchOHLCV` / `has.fetchTrades` 明确报告。下表中的“最早时间”不是 Ballast 承诺的固定日期，而是交易所当前公共端点实际返回的第一条记录；任务的 `covered_from` / `covered_to` 是本次真实覆盖范围。

| 交易所 | OHLCV | 公共逐笔成交 | 最早可回溯时间 | 主要限制 |
|---|---|---|---|---|
| Binance | 支持 | 支持 | 标的上线后且公共端点仍保留的首条记录 | 单页上限与地域可用性由 Binance 决定；当前开发出口可能返回 451 |
| OKX | 支持 | 支持 | OHLCV 以历史 K 线端点返回为准；成交通常是有限近期窗口 | 单页较小，需要连续分页；公共成交不宣称完整长期档案 |
| Bybit | 支持 | 支持 | 交易所返回的首条记录 | 当前开发出口可能返回 403；公共成交是有限窗口 |
| Gate.io | 支持 | 支持 | 交易所返回的首条记录 | 不同市场和周期的单页/时间范围限制不同 |
| Bitget | 支持 | 支持 | 交易所返回的首条记录 | K 线回溯深度随周期变化；公共成交是有限窗口 |

当交易所返回空页时，任务只记录已得到的覆盖范围，不把请求开始时间伪装为真实最早历史。查询 K 线会返回 `coverage` 和 `gaps`；分页时 `gap_scope` 明确说明缺口只基于当前返回页及首尾边界计算。

逐笔成交以毫秒时间戳分页。下一页会包含上一页页尾所在的毫秒，并依靠交易所成交 ID 幂等去重；不会把游标强行推进到 `+1ms`。如果交易所在同一毫秒返回满页且没有可安全使用的通用次级游标，下一页将无法推进，Ballast 以 `historical_cursor_not_advanced` 明确失败并保留此前已提交进度，避免静默漏掉成交。

完整历史 L2 订单簿通常没有公共回补端点。Ballast 仅能从启用实时采集后持续保存真实 L2；K 线也不能用于精确执行冲击回放。TWAP/POV 验证优先使用逐笔成交，需要盘口冲击时必须使用真实保存的 L2。

## 可复现示例

先同步标的，再创建一小时 OKX BTC/USDT K 线补数任务：

```bash
curl -X POST http://localhost:8080/api/v1/instruments/sync

curl -X POST http://localhost:8080/api/v1/history/backfills \
  -H 'content-type: application/json' \
  -d '{
    "exchange":"okx",
    "market_kind":"spot",
    "symbol":"BTC/USDT",
    "data_type":"ohlcv",
    "timeframe":"1m",
    "start_at":"2026-08-01T00:00:00Z",
    "end_at":"2026-08-01T01:00:00Z",
    "idempotency_key":"okx-btc-usdt-1m-20260801",
    "page_limit":100,
    "max_pages":25
  }'
```

用同一请求继续未完成任务不会重复写入。查询结果稳定按时间正序排列：

```bash
curl 'http://localhost:8080/api/v1/history/ohlcv?exchange=okx&market_kind=spot&symbol=BTC%2FUSDT&timeframe=1m&start_at=2026-08-01T00%3A00%3A00Z&end_at=2026-08-01T01%3A00%3A00Z&limit=500&offset=0'
```

逐笔成交请求把 `data_type` 改为 `trades` 并省略 `timeframe`；查询使用 `/api/v1/history/trades`。

## 策略验证回放

策略验证使用独立的 `ballast-backtest` 容器。它先通过服务端同步公开标的并补回 OHLCV 或逐笔成交，再把结果送入 Rust 回放器；整个流程只访问公共历史行情，不读取私有凭证，也不会调用创建、撤销或修改订单的接口。

回放的默认模型名为 `historical_volume_as_liquidity`：将每个时间片中的历史价格和成交量作为可用流动性，复用 TWAP/POV 策略与价格保护模拟 IOC。它不是 L2 订单簿重放，不能用来估计真实队列位置或精确冲击；需要盘口冲击验证时，必须使用已保存的真实 L2 数据。

在仓库根目录执行一小时 OKX BTC/USDT K 线回放：

```bash
BACKTEST_START_AT=2026-08-24T00:00:00Z \
BACKTEST_END_AT=2026-08-24T01:00:00Z \
BACKTEST_TARGET_AMOUNT=0.1 \
BACKTEST_STRATEGY=twap \
BACKTEST_DATA_TYPE=ohlcv \
BACKTEST_TIMEFRAME=1m \
make backtest
```

`trades` 通常只有有限的近期公共回溯窗口，使用它时应把时间范围改为刚结束的近期窗口；需要更长窗口时使用 `BACKTEST_DATA_TYPE=ohlcv` 并设置 `BACKTEST_TIMEFRAME=1m`。POV 需要额外设置 `BACKTEST_PARTICIPATION_RATE`，例如 `0.05`。命令输出包含完成度、残余量、平均价、最差价、滑点、手续费和逐 tick 结果，金额和数量均以十进制字符串输出。

常用参数：`BACKTEST_EXCHANGE`、`BACKTEST_MARKET_KIND`、`BACKTEST_SYMBOL`、`BACKTEST_SIDE`、`BACKTEST_QUANTITY_UNIT`、`BACKTEST_TICK_INTERVAL_SECONDS`、`BACKTEST_MAX_SLIPPAGE_BPS`、`BACKTEST_PAGE_LIMIT`、`BACKTEST_MAX_PAGES`、`BACKTEST_MAX_ROWS` 和 `BACKTEST_REQUEST_TIMEOUT_MS`。启用 OIDC 的服务需要给 `BACKTEST_BEARER_TOKEN` 一个至少具有 operator 权限的访问令牌。

批量比较五家交易所的 TWAP/POV 基线：

```bash
BACKTEST_START_AT=2026-08-23T00:00:00Z \
BACKTEST_END_AT=2026-08-23T01:00:00Z \
BACKTEST_TARGET_AMOUNT=0.1 \
BACKTEST_DATA_TYPE=ohlcv \
BACKTEST_TIMEFRAME=1m \
BACKTEST_TICK_INTERVAL_SECONDS=3600 \
BACKTEST_EXCHANGES=okx,binance,bybit,gate_io,bitget \
BACKTEST_STRATEGIES=twap,pov \
BACKTEST_MATRIX_POV_RATE=0.05 \
make backtest-matrix
```

矩阵默认顺序执行，避免同时压迫交易所公共限频；输出只包含可比较的汇总字段，单案例逐 tick 明细继续使用 `make backtest`。需要非笛卡尔组合时，可用 `BACKTEST_MATRIX_JSON` 传入案例数组；案例字段使用 `case_id`、`exchange`、`symbol`、`strategy`、`participation_rate` 等小写名称覆盖全局变量。某一交易所没有数据或回补失败时，该案例标记为 `failed`，矩阵整体标记为 `partial`，不会被计入完成案例。
