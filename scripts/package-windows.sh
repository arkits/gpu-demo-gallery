#!/bin/sh
# Cross-builds dist/windows/Zoltar.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin zoltar --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/zoltar.exe" "dist/windows/Zoltar.exe"
ls -l dist/windows
