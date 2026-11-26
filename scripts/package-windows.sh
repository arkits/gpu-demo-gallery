#!/bin/sh
# Cross-builds dist/windows/Wizards Tower.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin wizards-tower --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/wizards-tower.exe "dist/windows/Wizards Tower.exe"
ls -l dist/windows
