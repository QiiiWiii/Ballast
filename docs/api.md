# HTTP API

所有业务接口位于 `/api/v1`，价格、数量和金额字段使用十进制字符串。

## 历史行情

- `POST /api/v1/history/backfills`：创建或继续幂等补数任务
- `GET /api/v1/history/backfills/{job_id}`：读取游标、覆盖范围与失败原因
- `GET /api/v1/history/ohlcv`
- `GET /api/v1/history/trades`

补数参数包含 `exchange`、`market_kind`、`symbol`、`data_type`、`start_at`、`end_at` 和 `idempotency_key`。OHLCV 还必须提供 `timeframe`。`page_limit` 范围为 1–1000，`max_pages` 范围为 1–100；达到单次页数上限但尚未结束时任务返回 `pending`，用相同请求继续。完整示例与数据边界见 `docs/historical-data.md`。

## 标的与交易所

- `GET /api/v1/exchanges`
- `GET /api/v1/exchanges/snapshots`
- `GET /api/v1/exchanges/{exchange}`
- `GET /api/v1/exchanges/{exchange}/health-events`
- `GET /api/v1/exchanges/{exchange}/subscriptions`
- `GET /api/v1/instruments?exchange=binance&market_kind=spot&active_only=true&search=BTC&limit=100&offset=0`
- `POST /api/v1/instruments/sync`

标的接口始终分页返回 `{items,total,limit,offset}`，默认每页 100 条，单次最多 250 条。`search` 匹配统一 symbol、交易所原生 symbol、基础币和计价币；`ids` 接受逗号分隔 UUID，用于任务列表按需补齐标的信息。前端不得通过该接口一次下载完整标的库。

标的同步逐家返回结果。单家公网不可用不会把其他交易所的成功结果回滚：

```json
{
  "synchronized": {"okx": 1771, "gate_io": 3098, "bitget": 2040},
  "failed": {"binance": "gateway_unavailable", "bybit": "gateway_unavailable"}
}
```

失败的精确错误码通过交易所健康接口和健康事件查询；同步接口不返回上游错误正文。

## 执行任务

- `POST /api/v1/tasks`
- `GET /api/v1/tasks?limit=100`
- `GET /api/v1/tasks/{task_id}`
- `POST /api/v1/tasks/{task_id}/cancel`
- `GET /api/v1/tasks/{task_id}/slices`

创建任务必须提供 `Idempotency-Key`：

```json
{
  "instrument_id": "019...",
  "side": "sell",
  "target_amount": "1.25",
  "template_version_id": "019..."
}
```

策略、数量单位、持续时间、切片周期和保护参数全部取自模板版本。归档模板不能创建新任务。

## 策略模板

- `GET/POST /api/v1/strategy-templates`
- `GET /api/v1/strategy-templates/{template_id}`
- `POST /api/v1/strategy-templates/{template_id}/versions`
- `POST /api/v1/strategy-templates/{template_id}/archive`

版本为完整不可变快照。修改模板只能创建新版本，历史任务继续引用原版本。

模板通过 `scope` 区分 `paper_execution` 与 `research`。研究版本使用强类型 `algorithm_config`，案例金额、方向、时间窗和参与率上限不进入策略版本。研究版本不能创建执行任务。

模板必须显式提供 `execution_backend`：`managed_ioc` 或 `venue_native_algo`。native 模板还必须绑定交易所、市场类型和强类型 `native_configuration`。当前纸面任务只接受 managed 模板；native 模板不会被静默改为 managed。

## 策略研究

- `POST /api/v1/research/coverage`：检查 Alpaca Basic/IEX 在目标区间是否返回 TSLA 分钟数据。
- `GET/POST /api/v1/research/download-jobs`
- `GET /api/v1/research/download-jobs/{job_id}`
- `POST /api/v1/research/download-jobs/{job_id}/cancel`
- `GET /api/v1/validation-cases`
- `GET/PATCH /api/v1/validation-cases/{case_id}`：可改 `name` / `target_notional_usd` / `max_participation_rate` / `evaluation_sessions`（1–200）；保存创建新 case 版本，历史 run 继续使用各自的 `case_snapshot`。
- `GET/POST /api/v1/validation-runs`
- `GET /api/v1/validation-runs/compare?ids=`：比较 2–8 个 run 的聚合指标与最新决策；须先于 `{run_id}` 路由注册。
- `GET /api/v1/validation-runs/{run_id}`
- `POST /api/v1/validation-runs/{run_id}/cancel`
- `GET /api/v1/validation-runs/{run_id}/report`
- `GET /api/v1/validation-runs/{run_id}/export?format=json|csv&view=aggregates|sessions`：下载附件；`csv` 时 `view` 选择汇总或逐日。
- `GET/POST /api/v1/validation-runs/{run_id}/decisions`：人工 `validated` / `rejected`，备注最长 1000 字符，只追加不删除。

研究验证固定 TSLA、10:00–11:30 ET 卖出窗口、IEX 代理数据与四个固定研究版本（Immediate / TWAP / POV / VWAP）。下载任务仍固定 120 个交易日；案例 `evaluation_sessions` 可在 1–200 调整（warm-up 固定 20）。覆盖检查不产生数据费用，也不创建下载任务。所有报告携带 `data_quality=iex_proxy`；IEX 成交量不得解释为全美合并市场成交量。内部策略验证实验台不要求 OIDC。

下载状态为 `queued/downloading/verifying/completed/failed/cancelled`，验证状态为 `queued/preparing/running/succeeded/failed/cancelled`。取消和重复状态事件采用条件更新，不能让终态回到运行态。

## 原生算法与 live 安全状态

- `GET /api/v1/native-algorithms`
- `GET /api/v1/live/readiness`
- `GET /api/v1/accounts`
- `GET /api/v1/approvals`
- `GET /api/v1/risk`
- `GET /api/v1/hedges`

原生算法能力接口只返回经过研究的能力状态。`documented_not_validated` 不代表可提交。OIDC 和私有对账完成前，账户、审批、风险和对冲接口统一返回 HTTP 423 与 `live_execution_disabled`。

## 控制舱与分析

- `GET /api/v1/dashboard/operations?window=24h`
- `GET /api/v1/analytics/executions?window=7d&exchange=okx&strategy=twap`

允许窗口为 `24h`、`7d`、`30d`。跨标的指标使用任务完成比例和无权重滑点分位数，不把不同资产或数量单位直接相加。

交易所状态中的 `health_query_latency_ms` 表示 Rust 服务完成当前健康与能力探测的耗时，不代表交易所 REST 请求的往返延迟。列表接口执行五家聚合探测，单交易所详情只探测目标 adapter；`/exchanges/snapshots` 返回最后一次已持久化的当前探测结果，不触发网关请求。升级后尚未重新探测的记录返回 `null`，前端显示为“—”，不会伪装为 `0ms`。交易所原生请求延迟尚未进入正式契约。

## 事件

- `GET /api/v1/events?after_sequence=0&limit=500`
- `GET /api/v1/events?task_id=<uuid>&limit=1000`，读取单个任务最新的审计事件窗口，并按事件序号正序返回
- `GET /api/v1/ws`，升级为 WebSocket；首次连接从当前最新事件开始
- `GET /api/v1/ws?after_sequence=123`，从指定序号后补发断线期间事件

WebSocket 建立后首先发送 `stream_ready`，其中 `data.after_sequence` 是服务端确认的游标。客户端必须保存该游标，并在重连时显式传回；执行事件补发完成后应合并刷新页面数据，不能为每条历史事件分别触发全量请求。

`market_health` 是读取 PostgreSQL 健康快照的轻量心跳，不触发网关健康或能力重查。运营控制舱同样读取数据库快照，不因刷新运营指标而调用五家能力接口。切片低于最小交易限制、但任务总残余仍可继续执行时会写入 `slice_deferred` 事件，明确记录延后原因和最小限制。

错误示例：

```json
{"error":{"code":"quantity_unit_not_supported","params":{"quantity_unit":"contracts"}}}
```

服务端不返回拼接好的中英文错误文案。
