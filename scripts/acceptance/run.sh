#!/bin/sh
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
project_name=${COMPOSE_PROJECT_NAME:-ballast-acceptance}
network_name="${project_name}_ballast-private"

compose() {
    COMPOSE_PROJECT_NAME="$project_name" docker compose \
        -f "$root_dir/deploy/compose.yaml" \
        -f "$root_dir/deploy/compose.build.yaml" \
        -f "$root_dir/deploy/compose.acceptance.yaml" \
        "$@"
}

live_compose() {
    COMPOSE_PROJECT_NAME="$project_name" docker compose \
        -f "$root_dir/deploy/compose.yaml" \
        -f "$root_dir/deploy/compose.build.yaml" \
        -f "$root_dir/deploy/compose.acceptance.yaml" \
        -f "$root_dir/deploy/compose.acceptance.live.yaml" \
        "$@"
}

runner() {
    docker run --rm \
        --network "$network_name" \
        -e NODE_EXTRA_CA_CERTS=/certs/ca.crt \
        --mount type=bind,src="$root_dir",dst=/app,readonly \
        --mount type=volume,src=ballast-acceptance-certs,dst=/certs,readonly \
        --mount type=volume,src=ballast-acceptance-state,dst=/state \
        -w /app \
        node:22-bookworm-slim \
        node scripts/acceptance/run.mjs "$1"
}

export BALLAST_LIVE_ENABLED=false
export BALLAST_WEB_PORT=${BALLAST_ACCEPTANCE_WEB_PORT:-18080}
compose up -d --force-recreate --build acceptance-certs acceptance-oidc acceptance-webhook server web
compose run --rm --no-deps acceptance-seed
runner prepare
live_compose up -d --no-build server web
runner live

echo "acceptance environment is running on http://127.0.0.1:${BALLAST_ACCEPTANCE_WEB_PORT:-18080}"
