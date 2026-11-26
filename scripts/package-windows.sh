#!/bin/sh
# Cross-builds dist/windows/Valley of Ra.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin valley-of-ra --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/valley-of-ra.exe "dist/windows/Valley of Ra.exe"
ls -l dist/windows
