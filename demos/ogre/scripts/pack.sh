#!/bin/sh
# Packs model and texture data, and produces the browser's download manifest.
set -eu
cd "$(dirname "$0")/.."
data=${OGRE_DATA:-data}
out=$1
dest=$(dirname "$out")
manifest=$dest/assets.json
if [ -f "$out" ] && [ -f "$manifest" ] &&
   [ -z "$(find src scripts/pack.sh scripts/audio.list "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$dest"
while IFS= read -r name; do
  path=$(find "$data/audio" -type f -iname "$name.wav" | head -1)
  [ -n "$path" ] || { echo "missing sound: $name" >&2; exit 1; }
  ffmpeg -nostdin -v error -y -i "$path" -map_metadata -1 -compression_level 12 "$dest/$name.flac"
done < scripts/audio.list
cargo run -q --release --example pack -- "$data" target/package/data.pack
xz -T2 -6 -c --check=crc32 target/package/data.pack > "$out.tmp"
cargo run -q --release --example pack -- "$data" "$out.tmp" verify
mv "$out.tmp" "$out"
rm target/package/data.pack
python3 scripts/pack_parts.py "$out"
ls -l "$out"
