#!/bin/sh
# Cross-builds dist/windows/Subsurface Scattering.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p subsurface-scattering --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/subsurface-scattering.exe "dist/windows/Subsurface Scattering.exe"
ls -l dist/windows
