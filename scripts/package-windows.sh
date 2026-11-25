#!/bin/sh
# Cross-builds dist/windows/The Assassin.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p the-assassin --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/the-assassin.exe dist/windows/The Assassin.exe
ls -l dist/windows
