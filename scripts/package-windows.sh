#!/bin/sh
# Cross-builds dist/windows/Donut.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin donut --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/donut.exe "dist/windows/Donut.exe"
ls -l dist/windows
