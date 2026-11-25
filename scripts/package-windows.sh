#!/bin/sh
# Cross-builds dist/windows/Small Pond.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin pond --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/pond.exe "dist/windows/Small Pond.exe"
ls -l dist/windows
