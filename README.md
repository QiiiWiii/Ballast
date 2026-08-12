# Ballast（压舱石）

Ballast 是一个执行策略实验室，以及面向 Binance、OKX、Bybit、Gate.io 与 Bitget 的多交易所执行与对冲引擎。

项目采用混合架构：Rust 负责策略、执行状态机、风险控制、持久化与对冲编排；Node.js/ccxt 网关负责交易所协议、认证、限频和行情/订单流连接。两者通过版本化的 gRPC/Protobuf 契约通信。

> 当前状态：五家加密公共行情、版本化策略模板、纸面执行和运营工作台已实现；策略中心增加 TSLA $10M 卖出案例、Alpaca Basic/IEX 历史下载、确定性回放和四策略验证报告。私有账户与真实下单仍保持锁定。

## 目录

```text
.
├── crates/                  # Rust 核心 workspace
│   ├── ballast-core/        # 精确领域类型与公共错误
│   ├── ballast-strategies/  # TWAP / POV 策略
│   ├── ballast-execution/   # 执行状态机和调度边界
│   ├── ballast-simulator/   # 确定性模拟成交
│   ├── ballast-research/    # 股票策略回放、冲击模型和验证报告
│   ├── ballast-alpaca/      # Alpaca Basic / IEX 历史行情与本地缓存
│   ├── ballast-gateway-client/ # gRPC 网关客户端
│   ├── ballast-storage/     # PostgreSQL 持久化
│   └── ballast-server/      # axum 管理 API
├── gateway-node/            # Node.js/TypeScript + ccxt 交易所网关
├── proto/                   # Rust 与 Node 的唯一通信契约
├── migrations/              # PostgreSQL 迁移
├── deploy/                  # Docker 与 Compose 部署文件
├── docs/                    # 架构、开发和部署文档
└── web/                     # React 中英文运营与管理工作台
```

## 本地检查

依赖：Rust stable、Node.js 22+、npm 10+、Docker Compose。

```bash
make check
```

启动完整本地环境：

```bash
cp .env.example .env
docker compose -f deploy/compose.yaml up --build
```

打开 `http://localhost:8080`。股票验证需要在 `.env` 配置免费的 Alpaca market-data key；进入“策略中心 → 数据”检查覆盖并下载 120 个交易日，再在“实验”运行固定四策略比较。

更多信息见 [架构](docs/architecture.md)、[HTTP API](docs/api.md) 和 [交易所能力矩阵](docs/exchange-capabilities.md)。

## 安全边界

- 不得接入交易所下单 API Key。Alpaca Key 仅用于免费 IEX 行情读取，不授予下单能力。
- API Key 不得写入仓库、数据库、日志或错误信息。
- Node 网关不拥有策略、任务状态和风险决策。
- 任何状态不确定的下单请求都必须先对账，不得盲目重试。
- `BALLAST_LIVE_ENABLED` 默认 false；当前实现检测到 true 会拒绝启动。

## 文档与社区

- 贡献指南：[CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则：[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全政策：[SECURITY.md](SECURITY.md)
- 开源准备核对：[docs/open-source-readiness.md](docs/open-source-readiness.md)
- 第三方许可证摘要：[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)

## License

本项目以 [Apache License 2.0](LICENSE) 发布。

