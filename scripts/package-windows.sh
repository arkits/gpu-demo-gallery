#!/bin/sh
# Cross-builds dist/windows/Racing.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin racing --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/racing.exe "dist/windows/Racing.exe"
ls -l dist/windows
