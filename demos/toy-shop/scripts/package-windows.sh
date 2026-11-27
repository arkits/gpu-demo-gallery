#!/bin/sh
# Cross-builds dist/windows/Toy Shop.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p toy-shop --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/toy-shop.exe "dist/windows/Toy Shop.exe"
ls -l dist/windows
