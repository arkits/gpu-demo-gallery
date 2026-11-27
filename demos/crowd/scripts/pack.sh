#!/bin/sh
# Packs the demo's files losslessly into target/package/local/, which the browser
# version downloads as /local/: the archive in parts that fit Cloudflare Pages, listed
# in assets.json, with the intro and loop music and the logo. Needs xz and python3, and ffmpeg for the sound.
# The xz settings suit 32-bit floats; every file is compared with the original.
set -eu
cd "$(dirname "$0")/.."
SUSHI_ATI=${SUSHI_ATI:-data/Sushi.ati}
demo=$(dirname "$SUSHI_ATI")
out=target/package/local
tmp=target/package/local.tmp
if [ -f "$out/assets.json" ] && [ -z "$(find "$SUSHI_ATI" "$demo/Audio" "$demo"/Logo2.* \
  scripts/pack.sh scripts/pack_parts.py crates/sushi-formats/src/archive.rs -newer "$out/assets.json" -type f | head -1)" ]; then
  exit 0
fi
rm -rf "$tmp"
mkdir -p "$tmp/Audio"
pack() { cargo run -q --release -p sushi-formats --example pack -- "$@"; }
pack "$SUSHI_ATI" target/package/crowd.pack
xz -T1 -c --check=crc32 --lzma2=preset=9e,lc=2,lp=2,pb=2 target/package/crowd.pack > "$tmp/crowd.xz"
rm target/package/crowd.pack
pack "$SUSHI_ATI" "$tmp/crowd.xz" verify
python3 scripts/pack_parts.py "$tmp/crowd.xz"
# The parts replace the whole file.
[ ! -f "$tmp/crowd-00.part" ] || { cat "$tmp"/*.part | cmp - "$tmp/crowd.xz"; rm "$tmp/crowd.xz"; }
cp "$demo"/Logo2.[tT][gG][aA] "$tmp/Logo2.tga"
for part in Intro Loop; do
  ffmpeg -nostdin -v error -y -i "$demo/Audio/Scene$part.wav" -map_metadata -1 -compression_level 12 "$tmp/Audio/Scene$part.flac"
done
rm -rf "$out"
mv "$tmp" "$out"
ls -l "$out"
