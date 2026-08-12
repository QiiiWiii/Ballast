# Contributing

感谢关注 Ballast。开始修改前请先阅读 [AGENTS.md](AGENTS.md)、[docs/architecture.md](docs/architecture.md)
和本文件。

## 行为准则

参与本项目即表示你同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 开发环境

- Rust 1.88.0（`rust-toolchain.toml`）
- Node.js 22+、npm 10+
- Docker / Docker Compose

初始化：

```bash
cp .env.example .env
npm --prefix gateway-node ci
npm --prefix web ci
```

提交前至少运行：

```bash
make check
```

## 分支与 PR

1. 从 `main` 创建短生命周期分支。
2. 每次 PR 只解决一个明确问题；不要混入无关重构。
3. 为状态迁移、精度规则、协议变化和任务/订单状态机补充测试。
4. 在 [CHANGELOG.md](CHANGELOG.md) 的 `Unreleased` 中记录用户可见变化。
5. PR 描述写清动机、风险边界和未验证项。

## 工程边界

- Rust 是业务事实源；Node/ccxt 网关只做交易所通信与规范化。
- `proto/` 是跨语言唯一正式契约，禁止暴露无类型通用调用。
- 价格、数量、金额和手续费禁止使用二进制浮点数。
- 外部创建订单不允许盲目重试；超时必须进入不确定状态并按 `client_order_id` 对账。
- 不支持的交易所能力必须显式失败，禁止静默降级。

## 安全相关贡献

- **禁止**在 PR、issue、日志样例中提交真实 API Key、证书、账户信息或客户数据。
- 触及 live 下单、密钥读取、审批、风控或对冲路径的改动，必须单独开 PR，并在描述中标明安全审查点。
- 当前仓库默认 `BALLAST_LIVE_ENABLED=false`；不得以“方便演示”为由绕过 live 锁。

## 许可证

贡献默认按 [Apache License 2.0](LICENSE) 授权。提交 PR 即表示你有权按该许可证贡献代码，
并同意你的贡献以相同许可证发布。
