#!/bin/sh
# Cross-builds dist/windows/NPRHatching.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p nprhatching --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/nprhatching.exe dist/windows/NPRHatching.exe
ls -l dist/windows
