#!/bin/sh
# Packs the files the demo reads, and the sounds, and writes the browser's download
# manifest. Needs the demo in data/ (or GAS_STATION_DATA) with asm/ from
# tools/nv_compile.py.
#   scripts/pack.sh target/package/gas-station.xz
set -eu
cd "$(dirname "$0")/.."
data=${GAS_STATION_DATA:-data}
out=$1
dest=$(dirname "$out")
manifest=$dest/assets.json
if [ -f "$out" ] && [ -f "$manifest" ] &&
   [ -z "$(find -L crates scripts/pack.sh "$data/" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$dest"
for name in GasBMG3 Windy2m2; do
  path=$(find -L "$data/" -type f -ipath "*/sounds/$name.wav" | head -1)
  [ -n "$path" ] || { echo "missing sound: $name" >&2; exit 1; }
  ffmpeg -nostdin -v error -y -i "$path" -map_metadata -1 -compression_level 12 "$dest/$name.flac"
done
cargo run -q --release --example pack -- "$data" target/package/data.pack
xz -T0 -6 -c --check=crc32 target/package/data.pack > "$out.tmp"
cargo run -q --release --example pack -- "$data" "$out.tmp" verify
mv "$out.tmp" "$out"
rm target/package/data.pack
python3 scripts/pack_parts.py "$out"
ls -l "$out"
