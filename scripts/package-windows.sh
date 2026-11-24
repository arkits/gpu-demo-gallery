#!/bin/sh
# Cross-builds dist/windows/PipeDream.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p pipedream --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/pipedream.exe dist/windows/PipeDream.exe
ls -l dist/windows
