#!/bin/sh
# Cross-builds dist/windows/Double Cross.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p double-cross --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/double-cross.exe "dist/windows/Double Cross.exe"
ls -l dist/windows
