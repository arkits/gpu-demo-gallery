#!/bin/sh
# Cross-builds dist/windows/Bear.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p bear --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/bear.exe dist/windows/Bear.exe
ls -l dist/windows
