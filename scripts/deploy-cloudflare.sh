#!/bin/sh
# Deploys dist/ publicly to Cloudflare Pages: scripts/deploy-cloudflare.sh [project]
set -eu
cd "$(dirname "$0")/.."
WRANGLER="npx --yes wrangler@4.168.0"
project=${1:-gpu-demo-gallery}
sh scripts/build.sh >/dev/null
git check-ignore -q dist/img/creature.jpg || { echo "refusing: dist is not git-ignored" >&2; exit 1; }
if ! $WRANGLER pages project list 2>/dev/null | grep -qw "$project"; then
  # --force: otherwise wrangler creates a Workers project for new static sites.
  $WRANGLER pages project create "$project" --production-branch main --force
fi
$WRANGLER pages deploy dist --project-name "$project" --branch main --commit-dirty=true
