#!/bin/sh
# Cross-builds dist/windows/Grass.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin grass --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/grass.exe dist/windows/Grass.exe
ls -l dist/windows
