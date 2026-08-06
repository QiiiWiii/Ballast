# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) 的结构。正式发布后采用语义化版本。

## [Unreleased]

### Added

- 初始化 Rust 核心、Node/ccxt 网关、Protobuf 契约、PostgreSQL 迁移和部署骨架。
- 增加架构、开发、部署、安全及协作约束文档。
- 增加 Rust、Node 和 Compose 的持续集成检查。
- 将网关契约收敛为公共 `MarketDataService`，统一五家交易所的标的、盘口及逐笔成交。
- 增加现货、线性/反向永续领域模型，以及 base、quote、contracts 三种显式任务单位。
- 增加标的缓存、交易所健康、纸面切片和恢复调度迁移。
- 增加公共行情驱动的纸面 TWAP/POV、价格保护 IOC、残余保留和事件序号补发。
- 增加 `/api/v1` REST、WebSocket、Prometheus 指标和 PostgreSQL repository 集成测试。
- 增加中英文 React 控制舱：管理概览、运营执行台、任务证据和市场能力页。
- 增加 nginx 单入口、内部数据库/gRPC、Prometheus 与定时备份 Compose 服务。
- 增加系统共享策略模板、不可变版本、归档和现有任务参数迁移。
- 增加运营控制舱、执行质量与运营效率分析、交易所详情和真实订阅状态接口。
- 将前端重构为执行任务中心的信息架构，新增策略中心、执行分析、系统状态和后续能力页面。
- 增加 `AccountService`、`TradingService`、`AlgorithmicTradingService` 强类型 Protobuf 契约和 Rust 薄客户端。
- 增加 live 任务审批、多账户元数据、订单/原生算法、零默认风控、对冲和 WS ticket 的 `0004` 迁移。
- 增加审批、账户对账、风险和对冲锁舱页面；未配置私有能力时不展示伪数据或生产控件。
- 增加 Binance/OKX 原生算法研究能力接口，Bybit/Gate/Bitget 保持显式待调研状态。

### Changed

- Node 使用 Protobuf 生成的 TypeScript 类型，`ccxt` 固定为 `4.5.58`。
- 执行状态收敛为 `scheduled/running/paused/cancelling/completed/cancelled/expired/failed`。
- 任务创建契约改为强制引用 `template_version_id`，移除直接提交策略参数的旧契约。
- 移除旧 `/operations`、`/tasks/{id}` 和 `/markets` 前端路由。
- 将 managed 切片决策与执行状态边界迁入 `ballast-execution`，server worker 只装配行情、持久化和调度。
- 精确固定实际发布的 `barter-instrument 0.3.1`；原计划中的 `0.11.0` 在 crates.io 不存在。
