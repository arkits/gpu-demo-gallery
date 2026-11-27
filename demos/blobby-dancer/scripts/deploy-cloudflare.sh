#!/bin/sh
# Deploys the browser version to Cloudflare Pages with the demo files bundled,
# publicly. They are packed into dist/ (git-ignored), uploaded directly, and never
# enter the repository.
#   scripts/deploy-cloudflare.sh [project]      --prepare only assembles it
# Needs `npx wrangler login` once, or CLOUDFLARE_API_TOKEN.
set -eu
cd "$(dirname "$0")/.."
WRANGLER="npx --yes wrangler@4.168.0"
prepare=
if [ "${1:-}" = --prepare ]; then prepare=1; shift; fi
project=${1:-blobby-dancer-port}
site=dist/cloudflare
sh scripts/build-web.sh >/dev/null
sh scripts/pack.sh target/package/blobby-dancer.xz
rm -rf "$site"
mkdir -p "$site/local"
cp -R dist/pkg dist/index.html "$site/"
cp target/package/blobby-dancer.xz "$site/local/"
for f in target/package/*.flac; do [ ! -f "$f" ] || cp "$f" "$site/local/"; done
# Without this, Pages answers unknown paths with index.html.
echo '<!doctype html><title>Not found</title><p>Not found.' > "$site/404.html"
cat > "$site/_headers" <<'H'
/local/*
  Cache-Control: public, max-age=604800
/pkg/*
  Cache-Control: public, max-age=0, must-revalidate
H
big=$(find "$site" -type f -size +26214400c)
[ -z "$big" ] || { echo "over the 25 MiB Pages limit: $big" >&2; exit 1; }
# Never let the content be picked up by git, even with a forced add.
git check-ignore -q "$site/local/blobby-dancer.xz" || { echo "refusing: $site is not git-ignored" >&2; exit 1; }
[ -z "$prepare" ] || { du -sh "$site"; exit 0; }
if ! $WRANGLER pages project list 2>/dev/null | grep -qw "$project"; then
  # --force: otherwise wrangler creates a Workers project for new static sites.
  $WRANGLER pages project create "$project" --production-branch main --force
fi
$WRANGLER pages deploy "$site" --project-name "$project" --branch main --commit-dirty=true
