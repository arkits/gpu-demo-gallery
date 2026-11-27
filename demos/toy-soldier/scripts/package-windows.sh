#!/bin/sh
# Cross-builds dist/windows/Toy Soldier.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin toy_soldier --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/toy_soldier.exe "dist/windows/Toy Soldier.exe"
ls -l dist/windows
