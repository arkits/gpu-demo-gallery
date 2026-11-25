#!/bin/sh
# Cross-builds dist/windows/Flip.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin flip --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/flip.exe "dist/windows/Flip.exe"
ls -l dist/windows
