#!/bin/sh
# Cross-builds dist/windows/Lightning.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin lightning --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/lightning.exe "dist/windows/Lightning.exe"
ls -l dist/windows
