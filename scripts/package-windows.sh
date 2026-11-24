#!/bin/sh
# Cross-builds dist/windows/BoxOfSmoke.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin box-of-smoke --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/box-of-smoke.exe dist/windows/BoxOfSmoke.exe
ls -l dist/windows
