#!/bin/sh
# Cross-builds dist/windows/CarPaint.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p carpaint --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/carpaint.exe dist/windows/CarPaint.exe
ls -l dist/windows
