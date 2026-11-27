#!/bin/sh
# Cross-builds dist/windows/NaturalLight.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p naturallight --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/naturallight.exe dist/windows/NaturalLight.exe
ls -l dist/windows
