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

容器启动不代表每家交易所公网功能都可用。当前网关已实现五家公共标的、REST 盘口、WebSocket 盘口和逐笔成交；实际可用性仍受开发机出口地域、交易所限频和交易所维护状态影响。使用 `/api/v1/exchanges` 查看真实健康状态，不得把 `degraded` 静默改写为可用。

公共标的同步允许部分成功：

```bash
curl -X POST http://localhost:8080/api/v1/instruments/sync \
  -H 'Content-Type: application/json' \
  -d '{"reload":true}'
```

纸面执行会读取任务绑定交易所的实时公共盘口，但不会调用任何真实下单接口。
