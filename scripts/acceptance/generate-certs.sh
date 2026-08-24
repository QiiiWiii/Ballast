#!/bin/sh
set -eu

if [ -f /certs/ready ]; then
    exit 0
fi

apk add --no-cache openssl >/dev/null
rm -rf /certs/* /tmp/acceptance-*
mkdir -p /certs

openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout /certs/ca.key \
    -out /certs/ca.crt \
    -days 2 \
    -subj "/CN=Ballast Acceptance CA"

openssl req -newkey rsa:2048 -nodes \
    -keyout /certs/oidc.key \
    -out /tmp/acceptance-oidc.csr \
    -subj "/CN=acceptance-oidc"
printf '%s\n' 'subjectAltName=DNS:acceptance-oidc' > /tmp/acceptance-oidc.ext
openssl x509 -req \
    -in /tmp/acceptance-oidc.csr \
    -CA /certs/ca.crt \
    -CAkey /certs/ca.key \
    -CAcreateserial \
    -CAserial /tmp/acceptance-ca.srl \
    -out /certs/oidc.crt \
    -days 2 \
    -sha256 \
    -extfile /tmp/acceptance-oidc.ext

openssl req -newkey rsa:2048 -nodes \
    -keyout /certs/webhook.key \
    -out /tmp/acceptance-webhook.csr \
    -subj "/CN=acceptance-webhook"
printf '%s\n' 'subjectAltName=DNS:acceptance-webhook' > /tmp/acceptance-webhook.ext
openssl x509 -req \
    -in /tmp/acceptance-webhook.csr \
    -CA /certs/ca.crt \
    -CAkey /certs/ca.key \
    -CAserial /tmp/acceptance-ca.srl \
    -out /certs/webhook.crt \
    -days 2 \
    -sha256 \
    -extfile /tmp/acceptance-webhook.ext

chmod 600 /certs/*.key
touch /certs/ready
