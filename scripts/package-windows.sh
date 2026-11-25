#!/bin/sh
# Cross-builds dist/windows/Chameleon.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin chameleon --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/chameleon.exe" "dist/windows/Chameleon.exe"
ls -l dist/windows
