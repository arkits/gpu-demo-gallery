#!/bin/sh
# Packs the user's demo files losslessly: scripts/pack.sh OUT.xz, with the sounds as
# FLAC beside it. Needs xz and ffmpeg.
set -eu
cd "$(dirname "$0")/.."
data=${TOY_SOLDIER_DATA:-data}
out=$1
if [ -f "$out" ] && [ -z "$(find src -newer "$out" -type f | head -1)" ] && [ -z "$(find "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$(dirname "$out")"
for s in march panic scary; do
  ffmpeg -v error -y -i "$data/audio/$s.wav" -map_metadata -1 -compression_level 12 "$(dirname "$out")/$s.flac"
done
cargo run -q --release --example pack -- "$data" target/package/data.pack
xz -T1 -9e -c --check=crc32 target/package/data.pack > "$out.tmp"
cargo run -q --release --example pack -- "$data" "$out.tmp" verify
mv "$out.tmp" "$out"
rm target/package/data.pack
ls -l "$out"
