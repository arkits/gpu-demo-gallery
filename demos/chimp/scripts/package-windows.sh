#!/bin/sh
# Cross-builds dist/windows/Chimp.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p chimp --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/chimp.exe dist/windows/Chimp.exe
ls -l dist/windows
