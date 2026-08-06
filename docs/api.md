# HTTP API

所有业务接口位于 `/api/v1`，价格、数量和金额字段使用十进制字符串。

## 标的与交易所

- `GET /api/v1/exchanges`
- `GET /api/v1/exchanges/{exchange}`
- `GET /api/v1/exchanges/{exchange}/health-events`
- `GET /api/v1/exchanges/{exchange}/subscriptions`
- `GET /api/v1/instruments?exchange=binance&market_kind=spot&active_only=true`
- `POST /api/v1/instruments/sync`

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

模板必须显式提供 `execution_backend`：`managed_ioc` 或 `venue_native_algo`。native 模板还必须绑定交易所、市场类型和强类型 `native_configuration`。当前纸面任务只接受 managed 模板；native 模板不会被静默改为 managed。

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

## 事件

- `GET /api/v1/events?after_sequence=0&limit=500`
- `GET /api/v1/ws?after_sequence=0`，升级为 WebSocket

错误示例：

```json
{"error":{"code":"quantity_unit_not_supported","params":{"quantity_unit":"contracts"}}}
```

服务端不返回拼接好的中英文错误文案。
