#!/usr/bin/env bash
# Uploads release files to a PRIVATE Supabase Storage bucket, so downloads can
# later be served only to signed-in users (via short-lived signed URLs).
# Skips quietly when Supabase isn't configured yet.
#
# Usage: scripts/upload-to-storage.sh <channel> <version> <files...>
# Env:   SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY (CI secret only, never in the app),
#        STORAGE_BUCKET (default: releases)
# Each file goes to <channel>/<version>/<name> and <channel>/latest/<name>.
set -euo pipefail

channel="$1"; version="$2"; shift 2
bucket="${STORAGE_BUCKET:-releases}"

if [ -z "${SUPABASE_URL:-}" ] || [ -z "${SUPABASE_SERVICE_ROLE_KEY:-}" ]; then
  echo "::notice::SUPABASE_URL / SUPABASE_SERVICE_ROLE_KEY not set; skipping private download storage."
  exit 0
fi

for file in "$@"; do
  name="$(basename "$file")"
  for path in "$channel/$version/$name" "$channel/latest/$name"; do
    echo "Uploading $name -> $bucket/$path"
    curl -sSf -X POST "$SUPABASE_URL/storage/v1/object/$bucket/$path" \
      -H "Authorization: Bearer $SUPABASE_SERVICE_ROLE_KEY" \
      -H "x-upsert: true" \
      -H "Content-Type: application/octet-stream" \
      --data-binary "@$file" > /dev/null
  done
done
