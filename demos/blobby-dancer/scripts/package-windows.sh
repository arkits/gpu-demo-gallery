#!/bin/sh
# Cross-builds dist/windows/Blobby Dancer.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin blobby-dancer --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/blobby-dancer.exe "dist/windows/Blobby Dancer.exe"
ls -l dist/windows
