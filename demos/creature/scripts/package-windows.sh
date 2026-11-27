#!/bin/sh
# Cross-builds dist/windows/Creature.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin creature --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/creature.exe dist/windows/Creature.exe
ls -l dist/windows
