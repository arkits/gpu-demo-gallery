#!/bin/sh
# Cross-builds dist/windows/Surf.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin surf --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/surf.exe "dist/windows/Surf.exe"
ls -l dist/windows
