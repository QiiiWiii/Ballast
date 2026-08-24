# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) 和语义化版本。

## [Unreleased]

### Added

- 增加 Docker 内公开历史行情策略回放 runner，支持 OHLCV/逐笔成交驱动的 TWAP/POV 验证，并输出十进制执行报告；回放流程不调用真实下单接口。
- 增加 OKX 只读账户 gRPC smoke 命令，显式验证账户快照契约且只输出余额、仓位和未结订单数量，不输出金额或凭证。
- 增加对账差异 HTTPS webhook 告警；复用 outbox，按差异指纹去重并支持租约、退避重试和失败状态。
- 增加 OKX 受安全门控制的价格保护 IOC、撤单、私有订单/成交流和不确定提交状态。
- 增加 live 任务的三阶段风险检查、零默认限额、三级 kill switch、双人审批和关键路径 webhook 告警。
- 增加私有成交按交易所成交 ID 幂等落库，以及显式的手续费可用性状态。

## [0.1.1] - 2026-08-12

### Fixed

- OKX 的 50 档 WebSocket 订单簿订阅固定使用无需身份认证的公开 `books` 通道，并在网关内裁剪到请求深度，避免无 API Key 的 paper 部署持续触发认证错误。
- Gateway 健康响应从发布包元数据读取版本，生产镜像不再因缺少 npm 启动环境变量而报告旧版本。

## [0.1.0] - 2026-08-12

### Added

- 提供 Binance、OKX、Bybit、Gate.io 和 Bitget 公共标的、REST/WS 行情、健康状态与可恢复历史回补。
- 提供版本化策略模板、纸面 TWAP/POV、价格保护模拟 IOC、切片审计与 WebSocket 断线补发。
- 提供中英文 React 运营工作台，覆盖执行任务、策略、分析、市场通道与系统状态。
- 提供 PostgreSQL 权威状态、Prometheus 指标、定时备份和 Docker Compose 部署。
- 提供 `linux/amd64` 与 `linux/arm64` 多架构镜像发布工作流。
- 提供可选 OIDC 鉴权：JWKS 验签、`viewer/operator/admin` 角色、Web PKCE 和单次 WebSocket ticket。
- 提供 OKX 只读账户快照、按 `client_order_id` 查询与本地非终态订单周期对账 worker；不开放下单或撤单。
- 提供强类型私有交易与原生算法 Protobuf 契约，未验收能力保持失败关闭。

### Changed

- Rust 作为策略、任务、风控与持久化事实源；Node/ccxt 网关仅负责交易所通信与规范化。
- 任务创建强制引用不可变 `template_version_id`，移除直接提交策略参数的旧契约。
- 执行状态收敛为 `scheduled/running/paused/cancelling/completed/cancelled/expired/failed`。
- Node 使用 Protobuf 生成类型，`ccxt` 固定为 `4.5.58`。
- 采用 Apache License 2.0，并补齐贡献、安全、行为准则与开源准备文档。

### Fixed

- 公共标的同步允许单个交易所失败，并保留显式 degraded 状态。
- 单个市场无法满足精度契约时显式拒绝该市场，不再拖垮同一交易所的其余合法标的。
- 默认 paper 部署不再启动私有订单对账 worker，避免无密钥环境持续产生认证错误。
- 任务、切片和有序事件在同一事务中更新，避免终态事件丢失。
- 暂停重试、最小交易限制、标的分页与行情健康路径不再制造重复事件或伪数据。
- WebSocket 首连与重连使用确认游标，前端批量合并补发事件。
- 交易所详情、系统状态和控制舱使用持久化健康快照，避免在页面刷新时级联触发公网探测。

[Unreleased]: https://github.com/QiiiWiii/Ballast/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/QiiiWiii/Ballast/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/QiiiWiii/Ballast/releases/tag/v0.1.0
