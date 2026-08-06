.PHONY: check rust-check rust-test rust-fmt node-install node-check web-install web-check compose-config compose-up compose-down

check: rust-fmt rust-check rust-test node-check web-check compose-config

rust-fmt:
	cargo fmt --all -- --check

rust-check:
	cargo check --workspace --all-targets

rust-test:
	cargo test --workspace

node-install:
	npm --prefix gateway-node ci

node-check:
	npm --prefix gateway-node run check

web-install:
	npm --prefix web ci

web-check:
	npm --prefix web run check

compose-config:
	docker compose -f deploy/compose.yaml config --quiet

compose-up:
	docker compose -f deploy/compose.yaml up --build

compose-down:
	docker compose -f deploy/compose.yaml down
