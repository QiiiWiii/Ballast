#!/bin/sh
set -eu

issuer=${BALLAST_OIDC_ISSUER:-}
client_id=${BALLAST_OIDC_CLIENT_ID:-}
audience=${BALLAST_OIDC_AUDIENCE:-}

if { [ -n "$issuer" ] && [ -z "$client_id" ]; } || { [ -z "$issuer" ] && [ -n "$client_id" ]; }; then
    echo "BALLAST_OIDC_ISSUER and BALLAST_OIDC_CLIENT_ID must be set together" >&2
    exit 1
fi

for value in "$issuer" "$client_id" "$audience"; do
    if [ -n "$value" ] && ! printf '%s' "$value" | grep -Eq '^[A-Za-z0-9:/._?&=%+@~-]+$'; then
        echo "OIDC public configuration contains unsupported characters" >&2
        exit 1
    fi
done

cat > /usr/share/nginx/html/ballast-config.js <<EOF
window.__BALLAST_CONFIG__ = Object.freeze({
  oidcIssuer: "$issuer",
  oidcClientId: "$client_id",
  oidcAudience: "$audience"
});
EOF
