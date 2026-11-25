#!/bin/sh
# Cross-builds dist/windows/Borg.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin borg --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/borg.exe dist/windows/Borg.exe
ls -l dist/windows
