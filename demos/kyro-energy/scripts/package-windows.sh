#!/bin/sh
# Cross-builds dist/windows/KyroToon.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin kyro-energy --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/kyro-energy.exe "dist/windows/KyroToon.exe"
ls -l dist/windows
