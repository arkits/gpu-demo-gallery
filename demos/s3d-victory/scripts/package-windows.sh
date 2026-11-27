#!/bin/sh
# Cross-builds dist/windows/S3D Victory.exe. Needs mingw-w64 and the Rust target
# x86_64-pc-windows-gnu.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --bin s3d-victory --target x86_64-pc-windows-gnu
mkdir -p dist/windows
cp target/x86_64-pc-windows-gnu/release/s3d-victory.exe "dist/windows/S3D Victory.exe"
ls -l dist/windows
