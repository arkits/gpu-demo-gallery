#!/bin/sh
# Cross-builds dist/windows/RadeonsArk.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin radeons-ark --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/radeons-ark.exe "dist/windows/RadeonsArk.exe"
ls -l dist/windows
