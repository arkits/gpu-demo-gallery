#!/bin/sh
# Builds the browser version into dist/.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
bindgen=${WASM_BINDGEN:-target/tools/bin/wasm-bindgen}
if ! "$bindgen" --version 2>/dev/null | grep -qF "$version"; then
  cargo install --quiet --locked --root target/tools "wasm-bindgen-cli@$version"
  bindgen=target/tools/bin/wasm-bindgen
fi
cargo build --release --lib --target wasm32-unknown-unknown
mkdir -p dist
"$bindgen" --target web --no-typescript --out-dir dist/pkg \
  "${CARGO_TARGET_DIR:-target}"/wasm32-unknown-unknown/release/timbury.wasm
cp web/index.html dist/
ls -l dist/pkg/*.wasm
