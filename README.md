# Ballast（压舱石）

Ballast 是一个面向 Binance、OKX、Bybit、Gate.io 与 Bitget 的多交易所执行与对冲引擎。

项目采用混合架构：Rust 负责策略、执行状态机、风险控制、持久化与对冲编排；Node.js/ccxt 网关负责交易所协议、认证、限频和行情/订单流连接。两者通过版本化的 gRPC/Protobuf 契约通信。

> 当前状态：项目骨架。尚未接入真实交易所，也不具备真实下单能力。

## 目录

```text
.
├── crates/                  # Rust 核心 workspace
│   ├── ballast-core/        # 精确领域类型与公共错误
│   ├── ballast-strategies/  # TWAP / POV 策略
│   ├── ballast-execution/   # 执行状态机和调度边界
│   ├── ballast-simulator/   # 确定性模拟成交
│   ├── ballast-gateway-client/ # gRPC 网关客户端
│   ├── ballast-storage/     # PostgreSQL 持久化
│   └── ballast-server/      # axum 管理 API
├── gateway-node/            # Node.js/TypeScript + ccxt 交易所网关
├── proto/                   # Rust 与 Node 的唯一通信契约
├── migrations/              # PostgreSQL 迁移
├── deploy/                  # Docker 与 Compose 部署文件
├── docs/                    # 架构、开发和部署文档
└── web/                     # 前端预留目录
```

## 本地检查

依赖：Rust stable、Node.js 22+、npm 10+、Docker Compose。

```bash
make check
```

启动本地 PostgreSQL、交易所网关和 Rust 服务：

```bash
cp .env.example .env
docker compose -f deploy/compose.yaml up --build
```

服务入口：

- Rust API：`http://localhost:8080/health`
- Node gRPC 网关：`localhost:50051`
- PostgreSQL：`localhost:5432`

更多信息见 [docs/README.md](docs/README.md)。

## 安全边界

- 当前代码不得使用真实 API Key。
- API Key 不得写入仓库、数据库、日志或错误信息。
- Node 网关不拥有策略、任务状态和风险决策。
- 任何状态不确定的下单请求都必须先对账，不得盲目重试。
