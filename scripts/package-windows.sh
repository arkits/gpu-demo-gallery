#!/bin/sh
# Cross-builds dist/windows/Dangerous Curves.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p dangerous-curves --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/dangerous-curves.exe dist/windows/Dangerous Curves.exe
ls -l dist/windows
