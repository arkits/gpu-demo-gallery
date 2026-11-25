#!/bin/sh
# Packs the user's demo files losslessly: scripts/pack.sh OUT.xz. Needs xz.
set -eu
cd "$(dirname "$0")/.."
data=${SURF_DATA:-data}
out=$1
if [ -f "$out" ] && [ "$out" -nt src/data.rs ] && [ "$out" -nt src/demo.rs ] && [ -z "$(find "$data" -newer "$out" -type f | head -1)" ]; then exit 0; fi
mkdir -p target/package "$(dirname "$out")"
cargo run -q --release --example pack -- "$data" target/package/data.pack
xz -T1 -9e -c --check=crc32 target/package/data.pack > "$out.tmp"
cargo run -q --release --example pack -- "$data" target/package/data.pack verify
mv "$out.tmp" "$out"
cargo run -q --release --example pack -- "$data" "$out" verify
rm -r target/package/data.pack
ls -l "$out"
