#!/bin/sh
# Builds the browser version into dist/.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/version = "\(.*\)"/\1/p;}' Cargo.lock)
bindgen=target/tools/bin/wasm-bindgen
if ! "$bindgen" --version 2>/dev/null | grep -qF "$version"; then
  cargo install --quiet --locked --root target/tools "wasm-bindgen-cli@$version"
fi
cargo build --release --lib --target wasm32-unknown-unknown
mkdir -p dist
# The crate is `sixty-hertz`, so cargo emits `sixty_hertz.wasm`; keep the
# published JS name hyphenated to match web/index.html's import.
"$bindgen" --target web --no-typescript --out-dir dist/pkg \
  --out-name sixty-hertz \
  target/wasm32-unknown-unknown/release/sixty_hertz.wasm
cp web/index.html dist/
ls -l dist/pkg/*.wasm
