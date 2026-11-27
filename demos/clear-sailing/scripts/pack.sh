#!/bin/sh
# Packs the user's demo files losslessly in parts for the 25 MiB Pages limit:
# scripts/pack.sh, with the sounds as FLAC beside it. Needs xz, and ffmpeg
# for the sounds.
set -eu
cd "$(dirname "$0")/.."
data=${CLEAR_SAILING_DATA:-data}
mkdir -p target/package/wav target/package/packs
pack() { cargo run -q --release --example pack -- "$@"; }
pack "$data" target/package/wav sounds
for s in target/package/wav/*.wav; do
  [ -f "$s" ] || continue
  ffmpeg -v error -y -i "$s" -map_metadata -1 -compression_level 12 "target/package/$(basename "$s" .wav).flac"
done
pack "$data" target/package/packs
i=0
for p in target/package/packs/clear-sailing-*.pack; do
  out=target/package/clear-sailing-$i.xz
  if [ -f "$out" ] && [ "$out" -nt src/data.rs ] && [ "$out" -nt src/demo.rs ] && [ -z "$(find "$data" -newer "$out" -type f | head -1)" ]; then :; else
    xz -T1 -9e -c --check=crc32 "$p" > "$out.tmp"
    mv "$out.tmp" "$out"
  fi
  i=$((i + 1))
done
pack "$data" target/package/packs verify
ls target/package/packs/clear-sailing-*.pack | wc -l | tr -d ' ' > target/package/clear-sailing.parts
rm -r target/package/packs target/package/wav
ls -l target/package/
