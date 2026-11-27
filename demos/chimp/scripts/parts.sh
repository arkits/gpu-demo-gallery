#!/bin/sh
# Splits target/package/chimp.xz into parts that Cloudflare Pages accepts, with
# assets.json listing them, in target/package/local/.
set -eu
cd "$(dirname "$0")/.."
rm -rf target/package/local
mkdir -p target/package/local
cp target/package/chimp.xz target/package/local/
python3 scripts/pack_parts.py target/package/local/chimp.xz
[ ! -f target/package/local/chimp-00.part ] || rm target/package/local/chimp.xz
