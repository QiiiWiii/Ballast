.PHONY: check rust-check rust-clippy rust-test rust-fmt node-install node-check node-audit web-install web-check web-audit compose-config compose-up compose-down paper-smoke private-smoke

check: rust-fmt rust-check rust-clippy rust-test node-check node-audit web-check web-audit compose-config

rust-fmt:
	cargo fmt --all -- --check

rust-check:
	cargo check --workspace --all-targets

rust-clippy:
	cargo clippy --workspace --all-targets -- -D warnings

rust-test:
	cargo test --workspace

node-install:
	npm --prefix gateway-node ci

node-check:
	npm --prefix gateway-node run check

node-audit:
	npm --prefix gateway-node audit --audit-level=high

web-install:
	npm --prefix web ci

web-check:
	npm --prefix web run check

web-audit:
	npm --prefix web audit --audit-level=high

compose-config:
	docker compose -f deploy/compose.yaml config --quiet

compose-up:
	docker compose -f deploy/compose.yaml up --build

compose-down:
	docker compose -f deploy/compose.yaml down

paper-smoke:
	node scripts/paper-smoke.mjs

private-smoke:
	npm --prefix gateway-node run build
	npm --prefix gateway-node run private-smoke
