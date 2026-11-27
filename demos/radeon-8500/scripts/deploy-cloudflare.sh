#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
project=${1:-radeon-8500-port}
sh scripts/build-web.sh
big=$(find dist -type f -size +26214400c)
[ -z "$big" ] || { echo "Over the Pages file-size limit: $big" >&2; exit 1; }
projects=$(npx --yes wrangler@4.168.0 pages project list)
if ! printf '%s\n' "$projects" | grep -Fq "$project.pages.dev"; then
  npx --yes wrangler@4.168.0 pages project create "$project" --production-branch main --force
fi
npx --yes wrangler@4.168.0 pages deploy dist --project-name "$project" --branch main --commit-dirty=true
