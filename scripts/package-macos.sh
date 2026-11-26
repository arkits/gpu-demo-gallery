#!/bin/sh
# Builds dist/Racing.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Racing.app"
bin=target/package/racing
mkdir -p target/package
cargo build --release --bin racing --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin racing --target x86_64-apple-darwin
  lipo -create -output "$bin" target/aarch64-apple-darwin/release/racing \
    target/x86_64-apple-darwin/release/racing
else
  cp target/aarch64-apple-darwin/release/racing "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/racing"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Racing</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.racing</string>
  <key>CFBundleExecutable</key><string>racing</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
