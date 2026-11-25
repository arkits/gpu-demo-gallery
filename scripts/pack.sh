#!/bin/sh
# Repacks the user's Sushi.ati losslessly: scripts/pack.sh OUT.xz. Needs xz.
# The settings suit 32-bit floats; every file is compared with the original.
set -eu
cd "$(dirname "$0")/.."
SUSHI_ATI=${SUSHI_ATI:-data/Sushi.ati}
out=$1
if [ -f "$out" ] && [ "$out" -nt "$SUSHI_ATI" ] && [ "$out" -nt crates/sushi-formats/src/archive.rs ]; then
  exit 0
fi
mkdir -p target/package "$(dirname "$out")"
pack() { cargo run -q --release -p sushi-formats --example pack -- "$@"; }
pack "$SUSHI_ATI" target/package/the-assassin.pack
xz -T1 -c --check=crc32 --lzma2=preset=9e,lc=2,lp=2,pb=2 target/package/the-assassin.pack > "$out.tmp"
pack "$SUSHI_ATI" "$out.tmp" verify
mv "$out.tmp" "$out"
rm target/package/the-assassin.pack
