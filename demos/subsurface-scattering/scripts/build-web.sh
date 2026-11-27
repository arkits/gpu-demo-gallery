#!/bin/sh
# Builds the browser version into dist/.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
bindgen=target/tools/bin/wasm-bindgen
if ! "$bindgen" --version 2>/dev/null | grep -qF "$version"; then
  cargo install --quiet --locked --root target/tools "wasm-bindgen-cli@$version"
fi
cargo build --release --lib -p subsurface-scattering --target wasm32-unknown-unknown
mkdir -p dist
"$bindgen" --target web --no-typescript --out-dir dist/pkg \
  target/wasm32-unknown-unknown/release/subsurface_scattering.wasm
cp web/index.html dist/
ls -l dist/pkg/*.wasm
