#!/bin/sh
# Packs the user's demo files losslessly: scripts/pack.sh OUT.xz. Beside it go the music
# as FLAC (the original is WMA, which browsers do not play), the pack in parts of at
# most 24 MiB for Cloudflare Pages, and assets.json naming them. Needs xz and ffmpeg.
set -eu
cd "$(dirname "$0")/.."
data=${REEF_DATA:-data}
out=$1
dest=$(dirname "$out")
manifest=$dest/assets.json
if [ -f "$out" ] && [ -f "$manifest" ] &&
   [ -z "$(find -L src scripts/pack.sh scripts/pack_parts.py "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$dest"
rm -f "$dest"/*.flac "$dest"/*.part
music=$(find -L "$data" -type f -iname music.wma | head -1)
[ -n "$music" ] || { echo "missing music.wma" >&2; exit 1; }
ffmpeg -nostdin -v error -y -i "$music" -map_metadata -1 -compression_level 12 "$dest/music.flac"
cargo run -q --release --example pack -- "$data" target/package/data.pack
xz -T0 -9 -c --check=crc32 target/package/data.pack > "$out.tmp"
cargo run -q --release --example pack -- "$data" "$out.tmp" verify
mv "$out.tmp" "$out"
rm target/package/data.pack
python3 scripts/pack_parts.py "$out"
ls -l "$dest"
