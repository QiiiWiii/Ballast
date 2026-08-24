# Ballast 文档

- [系统架构](architecture.md)
- [HTTP API](api.md)
- [交易所能力矩阵](exchange-capabilities.md)
- [本地开发](development.md)
- [部署说明](deployment.md)
- [数据库边界](database.md)
- [历史行情与策略回放](historical-data.md)
- [前端产品与交互架构](frontend.md)
- [ADR-0001：Rust 核心与 Node/ccxt 网关](decisions/0001-rust-engine-node-gateway.md)
- [ADR：barter-instrument 集成边界](adr/0001-barter-instrument-boundary.md)
- [ADR：显式 managed/native 后端](adr/0002-explicit-execution-backends.md)
- [ADR：审批与零默认风控](adr/0003-live-approval-and-zero-default-risk.md)

当前仓库实现公共行情、纸面执行和共享环境；私有协议、数据模型与锁舱 UI 已建立，但私有账户、真实交易与对冲仍失败关闭。
