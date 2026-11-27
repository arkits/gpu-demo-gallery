#!/bin/sh
# Cross-builds dist/windows/Crowd.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p crowd --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/crowd.exe "dist/windows/Crowd.exe"
ls -l dist/windows
