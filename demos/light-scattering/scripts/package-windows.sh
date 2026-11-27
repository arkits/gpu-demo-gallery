#!/bin/sh
# Cross-builds dist/windows/Light Scattering.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin light-scattering --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/light-scattering.exe "dist/windows/Light Scattering.exe"
ls -l dist/windows
