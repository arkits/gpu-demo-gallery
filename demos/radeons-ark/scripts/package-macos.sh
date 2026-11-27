#!/bin/sh
# Builds dist/RadeonsArk.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/RadeonsArk.app"
bin=target/package/radeons-ark
mkdir -p target/package
cargo build --release --bin radeons-ark --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin radeons-ark --target x86_64-apple-darwin
  lipo -create -output "$bin" target/aarch64-apple-darwin/release/radeons-ark \
    target/x86_64-apple-darwin/release/radeons-ark
else
  cp target/aarch64-apple-darwin/release/radeons-ark "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/radeons-ark"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>RadeonsArk</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.radeons-ark</string>
  <key>CFBundleExecutable</key><string>radeons-ark</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
