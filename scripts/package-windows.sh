#!/bin/sh
# Cross-builds dist/windows/Grove.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin grove --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/grove.exe "dist/windows/Grove.exe"
ls -l dist/windows
