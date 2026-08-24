FROM rust:1.88.0-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY crates ./crates
COPY migrations ./migrations
COPY proto ./proto
RUN cargo build --locked --release -p ballast-backtest

FROM node:22-bookworm-slim

COPY --from=builder /app/target/release/ballast-backtest /usr/local/bin/ballast-backtest
COPY scripts/backtest.mjs /opt/ballast/backtest.mjs
COPY scripts/backtest-matrix.mjs /opt/ballast/backtest-matrix.mjs

USER node
ENTRYPOINT ["node", "/opt/ballast/backtest.mjs"]
