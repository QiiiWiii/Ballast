# Ballast（压舱石）

Ballast 是面向 Binance、OKX、Bybit、Gate.io 与 Bitget 的多交易所执行控制平台。

项目采用混合架构：Rust 负责策略、执行状态机、风险控制、持久化与对冲编排；Node.js/ccxt 网关负责交易所协议、认证、限频和行情/订单流连接。两者通过版本化的 gRPC/Protobuf 契约通信。

> 当前状态：五家加密公共行情、版本化策略模板、真实行情驱动的纸面执行、运营工作台，以及可恢复的交易所历史行情回补已实现。私有账户与真实下单仍保持锁定。

## 路线图

**已完成（paper 基线）**

- 五家公共行情：标的、盘口与成交同步，健康与 degraded 显式状态
- 版本化策略模板与纸面 TWAP / POV 执行
- 执行状态机、切片与有序事件、WebSocket 断线补发
- 运营控制舱、执行分析、市场与通道、系统状态
- 可恢复公共历史回补与 Compose 多架构镜像部署
- `v0.1` 正式发布轨道、版本化 Docker Hub 镜像与公开域名 paper smoke

**进行中（live P0 安全门槛，v0.2+）**

- OIDC 身份与权限：实现完成，待目标 provider 端到端验收
- 私有网关与密钥装配：OKX 只读账户快照与按客户端订单 ID 查询已接入
- 对账与不确定订单：订单状态机、逐单查询、账户级只读快照差异和脱敏 webhook 告警已接入

**规划中**

- P0.4 最小 live 执行（价格保护 IOC → live TWAP）
- P0.5 零默认风控与 kill switch、P0.6 审批与告警
- v0.3+ 深度能力：原生算法、多账户编排、成交驱动对冲

验收门槛、状态口径与非目标详见 [docs/roadmap.md](docs/roadmap.md)。

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
└── web/                     # React 中英文运营与管理工作台
```

## 本地检查

依赖：Rust 1.88、Node.js 22+、npm 10+、Docker Compose。

```bash
make check
```

启动完整本地环境：

```bash
cp .env.example .env
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

`deploy/compose.yaml` 只使用 Docker Hub 预构建多架构镜像（`amd64`/`arm64`）。镜像随 **`v*` 发版**发布：版本号 tag + 正式版 `latest`。本地源码构建用 `compose.build.yaml`。详见 [部署说明](docs/deployment.md)。

打开 `http://localhost:8080`。先在「市场与通道」同步标的，在「策略中心」创建模板，再创建纸面任务。公共行情与纸面执行不需要交易所下单 API Key。

更多信息见 [架构](docs/architecture.md)、[HTTP API](docs/api.md) 和 [交易所能力矩阵](docs/exchange-capabilities.md)。

## 安全边界

- 当前不得接入交易所下单 API Key；公共行情与纸面执行不依赖私有交易权限。
- API Key 不得写入仓库、数据库、日志或错误信息。
- Node 网关不拥有策略、任务状态和风险决策。
- 任何状态不确定的下单请求都必须先对账，不得盲目重试。
- `BALLAST_LIVE_ENABLED` 默认 `false`；只有 OIDC、私有对账、风险限额、kill switch、审批和告警前置条件全部满足时才允许设为 `true`，否则服务拒绝启动。

## 文档与社区

- 贡献指南：[CONTRIBUTING.md](CONTRIBUTING.md)
- 行为准则：[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- 安全政策：[SECURITY.md](SECURITY.md)
- 开源准备核对：[docs/open-source-readiness.md](docs/open-source-readiness.md)
- 路线图：[docs/roadmap.md](docs/roadmap.md)
- 第三方许可证摘要：[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)

## License

本项目以 [Apache License 2.0](LICENSE) 发布。
