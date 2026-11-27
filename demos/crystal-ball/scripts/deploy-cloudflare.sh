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
project=${1:-crystal-ball-port}
site=dist/cloudflare
sh scripts/build-web.sh >/dev/null
sh scripts/pack.sh target/package/crystal_ball.xz
rm -rf "$site"
mkdir -p "$site/local"
cp -R dist/pkg dist/index.html "$site/"
cp target/package/crystal_ball.xz target/package/*.flac "$site/local/"
# The pack is cached for a week, so a new one needs a new address.
version=$(shasum -a 256 target/package/crystal_ball.xz | cut -c1-12)
sed -i.bak "s|\"./local/crystal_ball.xz\"|\"./local/crystal_ball.xz?v=$version\"|" "$site/index.html"
rm "$site/index.html.bak"
grep -q "crystal_ball.xz?v=$version" "$site/index.html" || { echo "could not version the pack" >&2; exit 1; }
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
git check-ignore -q "$site/local/crystal_ball.xz" || { echo "refusing: $site is not git-ignored" >&2; exit 1; }
[ -z "$prepare" ] || { du -sh "$site"; exit 0; }
if ! $WRANGLER pages project list 2>/dev/null | grep -qw "$project"; then
  # --force: otherwise wrangler creates a Workers project for new static sites.
  $WRANGLER pages project create "$project" --production-branch main --force
fi
$WRANGLER pages deploy "$site" --project-name "$project" --branch main --commit-dirty=true
