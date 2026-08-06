# 交易所能力矩阵

锁定版本：`ccxt 4.5.58`。普通 PR CI 运行 fixture 合约测试；公网能力通过手动 smoke test 复核。

| 交易所 | ccxt ID | 现货 | 线性永续 | 反向永续 | REST 盘口 | WS 盘口 | WS 成交 | 沙盒下单 |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Binance | `binance` | 是 | 是 | 是 | 已接入 | 已接入 | 已接入 | 后续验证 |
| OKX | `okx` | 是 | 是 | 是 | 已接入 | 已接入 | 已接入 | 后续验证 |
| Bybit | `bybit` | 是 | 是 | 是 | 已接入 | 已接入 | 已接入 | 后续验证 |
| Gate.io | `gate` | 是 | 是 | 按公开市场实际结果 | 已接入 | 已接入 | 已接入 | 后续验证 |
| Bitget | `bitget` | 是 | 是 | 按公开市场实际结果 | 已接入 | 已接入 | 已接入 | ccxt 当前未声明 |

“已接入”表示统一适配代码与 fixture 测试完成，不表示当前开发机已完成公网验证。`ListInstruments` 以交易所实际返回为准，不合成不存在的市场。

2026-08-06 在当前开发出口完成公网 smoke test：

| 交易所 | 公共标的结果 | 当前出口状态 |
|---|---:|---|
| Binance | 失败 | 官方返回 HTTP 451 地域限制，Ballast 显示 degraded |
| OKX | 1771 | 成功，并以 BTC/USDT 现货完成实时盘口纸面 TWAP |
| Bybit | 失败 | CloudFront 返回 HTTP 403 地域限制，Ballast 显示 degraded |
| Gate.io | 3098 | 成功 |
| Bitget | 2040 | 成功 |

数量是该时点 ccxt 返回的现货与永续市场快照，会随交易所上下架变化。Ballast 不通过备用域名、代理或自动换所规避地域限制。

## 原生算法研究矩阵

| 交易所 | 算法 | 当前状态 | 开放条件 |
|---|---|---|---|
| Binance | Spot TWAP、USDⓈ-M TWAP、USDⓈ-M VP | 官方端点已确认，未完成私有生命周期验收 | 创建、查询、取消、子订单/成交对账、价格保护全部通过 |
| OKX | Spot/永续 TWAP | 官方端点已确认，未完成私有生命周期验收 | `szLimit`、`pxLimit`、`timeInterval`、`pxVar/pxSpread` 与成交对账全部通过 |
| Bybit | — | `research_required` | 未满足能力门槛前明确不支持 |
| Gate.io | — | `research_required` | 未满足能力门槛前明确不支持 |
| Bitget | — | `research_required` | 无可用沙盒时还需隔离子账户与显式生产授权 |

能力查询中的所有提交标志当前均为 false。官方文档存在不等于 Ballast 已获得真实提交授权。

## 公网 smoke test

每家至少检查现货和线性永续的标的、20 档盘口、盘口流、逐笔成交 ID/方向/数量、重连与 stale 状态。反向永续按实际能力验证，不把交割合约计入范围。
