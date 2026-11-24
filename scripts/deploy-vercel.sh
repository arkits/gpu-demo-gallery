#!/bin/sh
# Deploys the browser version to Vercel with the demo content bundled, publicly.
# The content is assembled in dist/ (git-ignored) and never enters the repository.
#   scripts/deploy-vercel.sh [project] [scope]     --prepare only assembles it
set -eu
cd "$(dirname "$0")/.."
DEMO="ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream"
prepare=
if [ "${1:-}" = --prepare ]; then prepare=1; shift; fi
project=${1:-pipedream}
scope=${2:-arkits}
site=dist/vercel
sh scripts/build-web.sh >/dev/null
# Keep .vercel, which links this folder to its project.
mkdir -p "$site/local/Audio"
rm -rf "$site/pkg"
cp -R dist/pkg dist/index.html "$site/"
cp "$DEMO/Sushi.ati" "$DEMO/Logo2.tga" "$site/local/"
cp "$DEMO/Audio/AnimusicIntro.mp3" "$site/local/Audio/"
cat > "$site/vercel.json" <<'J'
{
  "headers": [
    { "source": "/local/(.*)", "headers": [{ "key": "Cache-Control", "value": "public, max-age=31536000, immutable" }] },
    { "source": "/pkg/(.*)", "headers": [{ "key": "Cache-Control", "value": "public, max-age=0, must-revalidate" }] }
  ]
}
J
# Never let the content be picked up by git, even with a forced add.
git check-ignore -q "$site/local/Sushi.ati" || { echo "refusing: $site is not git-ignored" >&2; exit 1; }
[ -z "$prepare" ] || { du -sh "$site"; exit 0; }
cd "$site"
[ -d .vercel ] || vercel link --yes --project "$project" --scope "$scope"
vercel deploy --prod --yes --scope "$scope"
