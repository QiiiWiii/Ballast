# Third-party licenses

Ballast 本身以 [Apache License 2.0](LICENSE) 发布。以下为**直接依赖**的许可证摘要，
用于开源兼容性核对；完整传递依赖以 lockfile 与上游声明为准。

## 重点说明

### barter-instrument 0.3.1

- Project: <https://github.com/barter-rs/barter-rs>
- License: MIT
- Usage: 方向、命名、索引与数量单位等无损领域原语。
- Ballast 不使用 `barter-execution` 或其交易所客户端。

### ccxt（gateway-node）

- License: MIT
- Usage: 交易所协议适配；权威任务状态仍在 Rust 侧。

### 字体（web）

- `@fontsource/barlow-condensed`、`@fontsource/ibm-plex-sans`、`@fontsource/ibm-plex-mono`
- 字体文件通常遵循 **SIL Open Font License (OFL)**；与 Apache-2.0 项目分发兼容，保留上游字体版权与 OFL 声明。

## Rust workspace 直接依赖（常见许可证）

| Crate | 常见许可证 |
| --- | --- |
| async-trait | MIT OR Apache-2.0 |
| axum | MIT |
| barter-instrument | MIT |
| chrono / chrono-tz | MIT OR Apache-2.0 |
| futures-util | MIT OR Apache-2.0 |
| hex | MIT OR Apache-2.0 |
| prost | Apache-2.0 |
| prometheus | Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| rust_decimal | MIT |
| serde / serde_json | MIT OR Apache-2.0 |
| sha2 | MIT OR Apache-2.0 |
| sqlx | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| tokio | MIT |
| tonic | MIT |
| tower-http | MIT |
| tracing / tracing-subscriber | MIT |
| uuid | Apache-2.0 OR MIT |
| zstd | BSD / MIT family（以 crate 声明为准） |

未发现直接依赖要求整体项目使用 GPL/AGPL 的 copyleft 条款。升级依赖后应复查许可证字段。

## Node 直接依赖（常见许可证）

### gateway-node

| Package | 常见许可证 |
| --- | --- |
| @grpc/grpc-js | Apache-2.0 |
| @grpc/proto-loader | Apache-2.0 |
| ccxt | MIT |
| decimal.js | MIT |
| pino | MIT |

### web

| Package | 常见许可证 |
| --- | --- |
| react / react-dom | MIT |
| @tanstack/react-query | MIT |
| @tanstack/react-router | MIT |
| zod | MIT |
| recharts | MIT |
| i18next / react-i18next | MIT |
| @radix-ui/react-dialog | MIT |

## 生成与复核建议

发布前建议在干净环境执行依赖许可证清单复核，例如：

```bash
# Rust（需安装 cargo-license）
cargo install cargo-license
cargo license --workspace

# Node（需安装 license-checker 等工具）
npx --yes license-checker --production --summary --start gateway-node
npx --yes license-checker --production --summary --start web
