# Third-party licenses

Ballast directly depends on the following trading-domain project:

## barter-instrument 0.3.1

- Project: <https://github.com/barter-rs/barter-rs>
- License: MIT
- Usage: lossless direction, naming, indexing and quantity-unit primitives only.
- Version note: `0.11.0` is not published for this crate; Ballast pins the actual
  crates.io release `0.3.1` exactly and treats any upgrade as a compatibility change.

Ballast does not use `barter-execution` or its exchange clients. Exchange communication remains
behind the Node.js/ccxt gateway, and Ballast retains its optional precision and limit metadata so
that missing venue constraints are never represented as zero.
