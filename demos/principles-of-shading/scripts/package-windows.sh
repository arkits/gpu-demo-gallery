#!/bin/sh
# Cross-builds dist/windows/PrinciplesOfShading.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin principles-of-shading --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/principles-of-shading.exe dist/windows/PrinciplesOfShading.exe
ls -l dist/windows
