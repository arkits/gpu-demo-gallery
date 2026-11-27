#!/bin/sh
# Cross-builds dist/windows/Clear Sailing.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin clear-sailing --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/clear-sailing.exe "dist/windows/Clear Sailing.exe"
ls -l dist/windows
