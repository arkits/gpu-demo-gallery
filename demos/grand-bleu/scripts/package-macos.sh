#!/bin/sh
# Builds dist/Grand Bleu.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Grand Bleu.app"
bin=target/package/grand-bleu
mkdir -p target/package
cargo build --release --bin grand-bleu --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin grand-bleu --target x86_64-apple-darwin
  lipo -create -output "$bin" target/aarch64-apple-darwin/release/grand-bleu \
    target/x86_64-apple-darwin/release/grand-bleu
else
  cp target/aarch64-apple-darwin/release/grand-bleu "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/grand-bleu"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Grand Bleu</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.grand-bleu</string>
  <key>CFBundleExecutable</key><string>grand-bleu</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
