#!/bin/sh
# Packs the demo's files losslessly into target/package/local/, which the browser
# version downloads as /local/: the archive in parts that fit Cloudflare Pages, listed
# in assets.json, with the soundtrack and the logo. Needs xz and python3.
# The xz settings suit 32-bit floats; every file is compared with the original.
set -eu
cd "$(dirname "$0")/.."
SUSHI_ATI=${SUSHI_ATI:-data/Sushi.ati}
demo=$(dirname "$SUSHI_ATI")
out=target/package/local
tmp=target/package/local.tmp
if [ -f "$out/assets.json" ] && [ -z "$(find "$SUSHI_ATI" "$demo/Audio" "$demo/Logo2.tga" \
  scripts/pack.sh scripts/pack_parts.py crates/sushi-formats/src/archive.rs -newer "$out/assets.json" -type f | head -1)" ]; then
  exit 0
fi
rm -rf "$tmp"
mkdir -p "$tmp/Audio"
pack() { cargo run -q --release -p sushi-formats --example pack -- "$@"; }
pack "$SUSHI_ATI" target/package/double-cross.pack
xz -T1 -c --check=crc32 --lzma2=preset=9e,lc=2,lp=2,pb=2 target/package/double-cross.pack > "$tmp/double-cross.xz"
rm target/package/double-cross.pack
pack "$SUSHI_ATI" "$tmp/double-cross.xz" verify
python3 scripts/pack_parts.py "$tmp/double-cross.xz"
# The parts replace the whole file.
[ ! -f "$tmp/double-cross-00.part" ] || { cat "$tmp"/*.part | cmp - "$tmp/double-cross.xz"; rm "$tmp/double-cross.xz"; }
cp "$demo/Logo2.tga" "$tmp/"
cp "$demo/Audio/SceneIntro.mp3" "$tmp/Audio/"
rm -rf "$out"
mv "$tmp" "$out"
ls -l "$out"
