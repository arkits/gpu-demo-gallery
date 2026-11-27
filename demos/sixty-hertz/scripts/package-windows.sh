#!/bin/sh
# Cross-builds dist/windows/SixtyHz.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin sixty-hertz --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/sixty-hertz.exe "dist/windows/SixtyHz.exe"
ls -l dist/windows
