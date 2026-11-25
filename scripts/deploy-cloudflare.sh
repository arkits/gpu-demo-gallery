#!/bin/sh
# Publishes the static player. The page opens the user's own demo files.
set -eu
cd "$(dirname "$0")/.."
sh scripts/build-web.sh
mkdir -p target/site
cp -R dist/pkg dist/index.html target/site/
printf '%s\n' '<!doctype html><title>Not found</title><p>Not found.' > target/site/404.html
if [ "${1:-}" = --prepare ]; then exit 0; fi
project=${1:-double-cross-port}
wrangler() { npx --yes wrangler@4.168.0 "$@"; }
projects=$(wrangler pages project list)
if ! printf '%s\n' "$projects" | grep -qw "$project"; then
  wrangler pages project create "$project" --production-branch main --force
fi
wrangler pages deploy target/site --project-name "$project" --branch main --commit-dirty=true
