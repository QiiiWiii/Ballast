#!/bin/sh
set -eu

backup_dir=/backups
interval_seconds="${BACKUP_INTERVAL_SECONDS:-86400}"
retention_days="${BACKUP_RETENTION_DAYS:-14}"
mkdir -p "$backup_dir"

while true; do
  timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
  target="$backup_dir/ballast-$timestamp.dump"
  pg_dump --format=custom --no-owner --no-acl --file="$target" "$DATABASE_URL"
  find "$backup_dir" -type f -name 'ballast-*.dump' -mtime "+$retention_days" -delete
  sleep "$interval_seconds"
done
