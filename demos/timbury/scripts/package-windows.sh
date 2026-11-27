#!/bin/sh
# Cross-builds dist/windows/Timbury.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin timbury --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/timbury.exe" "dist/windows/Timbury.exe"
ls -l dist/windows
