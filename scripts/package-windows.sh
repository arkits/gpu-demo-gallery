#!/bin/sh
# Cross-builds dist/windows/ATB-TV.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin atb-tv --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/atb-tv.exe "dist/windows/ATB-TV.exe"
ls -l dist/windows
