#!/bin/sh
# Cross-builds dist/windows/Crystal Ball.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin crystal_ball --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/crystal_ball.exe "dist/windows/Crystal Ball.exe"
ls -l dist/windows
