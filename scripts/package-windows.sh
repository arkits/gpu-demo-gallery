#!/bin/sh
# Cross-builds dist/windows/Grand Bleu.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin grand-bleu --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/grand-bleu.exe "dist/windows/Grand Bleu.exe"
ls -l dist/windows
