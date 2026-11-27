#!/bin/sh
# Cross-builds dist/windows/Mad Mod Mike.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin mad-mod-mike --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-gnu/release/mad-mod-mike.exe" "dist/windows/Mad Mod Mike.exe"
ls -l dist/windows
