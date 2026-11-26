#!/bin/sh
# Packs the demo files the port needs losslessly: scripts/pack.sh OUT.xz.
# Needs xz. Only the three FILES are packed; nothing third-party is committed.
set -eu
cd "$(dirname "$0")/.."
data=${GRAND_BLEU_DATA:-data}
out=$1
if [ -f "$out" ] && [ "$out" -nt crates/grand-bleu/src/data.rs ] && [ "$out" -nt crates/grand-bleu/src/demo.rs ] && [ -z "$(find "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$(dirname "$out")"
pack() { cargo run -q --release -p grand-bleu --example pack -- "$@"; }
pack "$data" target/package/data.pack
xz -T1 -9e -c --check=crc32 target/package/data.pack > "$out.tmp"
pack "$data" target/package/data.pack verify
mv "$out.tmp" "$out"
pack "$data" "$out" verify
rm -r target/package/data.pack
ls -l "$out"
