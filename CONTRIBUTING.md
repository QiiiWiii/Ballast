# Contributing

开始修改前请先阅读 [AGENTS.md](AGENTS.md) 和 [docs/architecture.md](docs/architecture.md)。

## 开发流程

1. 从 `main` 创建短生命周期分支。
2. 保持每次修改只解决一个明确问题。
3. 为状态迁移、精度规则和协议变化添加测试。
4. 运行 `make check`。
5. 在 `CHANGELOG.md` 的 `Unreleased` 中记录用户可见变化。

真实交易能力必须经过单独的安全审查，不得与普通重构混在同一个变更中。
