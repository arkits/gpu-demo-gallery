#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release --lib --target wasm32-unknown-unknown
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
bindgen=${WASM_BINDGEN:-target/tools/bin/wasm-bindgen}
if ! "$bindgen" --version 2>/dev/null | grep -qF "$version"; then
  [ -z "${WASM_BINDGEN:-}" ] || { echo "WASM_BINDGEN must be version $version" >&2; exit 1; }
  cargo install --quiet --locked --root target/tools "wasm-bindgen-cli@$version"
fi
mkdir -p dist/pkg
"$bindgen" --target web --no-typescript --out-dir dist/pkg \
  target/wasm32-unknown-unknown/release/radeon_8500.wasm
cp web/index.html web/player.css web/player.js dist/
python3 scripts/stage-content.py
