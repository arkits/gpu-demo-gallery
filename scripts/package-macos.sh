#!/bin/sh
# Builds dist/Small Pond.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Small Pond.app"
bin=target/package/pond
mkdir -p target/package
cargo build --release --bin pond --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin pond --target x86_64-apple-darwin
  lipo -create -output "$bin" target/aarch64-apple-darwin/release/pond \
    target/x86_64-apple-darwin/release/pond
else
  cp target/aarch64-apple-darwin/release/pond "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/pond"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Small Pond</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.pond</string>
  <key>CFBundleExecutable</key><string>pond</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
