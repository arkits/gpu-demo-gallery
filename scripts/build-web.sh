#!/bin/sh
# Builds the browser version into dist/ — once web/index.html + WebFiles::open
# exist. The crate `valley-of-ra` emits `valley_of_ra.wasm`; keep the JS name matching.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
bindgen=target/tools/bin/wasm-bindgen
if ! "$bindgen" --version 2>/dev/null | grep -qF "$version"; then
  cargo install --quiet --locked --root target/tools "wasm-bindgen-cli@$version"
fi
cargo build --release --lib -p valley-of-ra --target wasm32-unknown-unknown
mkdir -p dist
"$bindgen" --target web --no-typescript --out-dir dist/pkg \
  --out-name valley-of-ra \
  target/wasm32-unknown-unknown/release/valley_of_ra.wasm
cp web/index.html dist/
ls -l dist/pkg/*.wasm
