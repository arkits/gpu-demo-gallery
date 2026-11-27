#!/bin/sh
# Cross-builds dist/windows/Wolfman.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin wolfman --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/wolfman.exe" "dist/windows/Wolfman.exe"
ls -l dist/windows
