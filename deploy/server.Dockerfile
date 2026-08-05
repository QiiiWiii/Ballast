FROM rust:1.88.0-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY crates ./crates
COPY migrations ./migrations
COPY proto ./proto
RUN cargo build --locked --release -p ballast-server

FROM debian:bookworm-slim AS runtime

RUN useradd --create-home --uid 10001 ballast
COPY --from=builder /app/target/release/ballast-server /usr/local/bin/ballast-server

USER ballast
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/ballast-server"]
