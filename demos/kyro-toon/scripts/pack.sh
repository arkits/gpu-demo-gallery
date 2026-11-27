#!/bin/sh
# Packs the user's demo files losslessly: scripts/pack.sh OUT.xz, with the sounds as
# FLAC beside it. Needs xz, and ffmpeg for the sounds.
set -eu
cd "$(dirname "$0")/.."
data=${TOON_DATA:-data}
out=$1
if [ -f "$out" ] && [ "$out" -nt src/data.rs ] && [ "$out" -nt src/demo.rs ] && [ -z "$(find "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package/wav "$(dirname "$out")"
pack() { cargo run -q --release --example pack -- "$@"; }
pack "$data" target/package/wav sounds
for s in target/package/wav/*.wav; do
  [ -f "$s" ] || continue
  ffmpeg -v error -y -i "$s" -map_metadata -1 -compression_level 12 "$(dirname "$out")/$(basename "$s" .wav).flac"
done
pack "$data" target/package/data.pack
xz -T1 -9e -c --check=crc32 target/package/data.pack > "$out.tmp"
pack "$data" target/package/data.pack verify
mv "$out.tmp" "$out"
pack "$data" "$out" verify
rm -r target/package/data.pack target/package/wav
ls -l "$out"
