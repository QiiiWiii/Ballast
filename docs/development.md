# 本地开发

## 环境

- Rust 1.88.0（由 `rust-toolchain.toml` 固定）
- Node.js 22 或更高版本
- npm 10 或更高版本
- Docker 与 Docker Compose

## 初始化

```bash
npm --prefix gateway-node install
cp .env.example .env
```

## 检查

```bash
make check
```

也可以单独运行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
npm --prefix gateway-node run check
docker compose -f deploy/compose.yaml config
```

## 启动

```bash
docker compose -f deploy/compose.yaml up --build
```

容器启动不代表交易所功能可用。当前网关只有健康检查，其他方法会返回 `UNIMPLEMENTED`。
