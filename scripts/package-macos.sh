#!/bin/sh
# Builds dist/Grove.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Grove.app"
bin=target/package/grove
mkdir -p target/package
cargo build --release --bin grove --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin grove --target x86_64-apple-darwin
  lipo -create -output "$bin" target/aarch64-apple-darwin/release/grove \
    target/x86_64-apple-darwin/release/grove
else
  cp target/aarch64-apple-darwin/release/grove "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/grove"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Grove</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.grove</string>
  <key>CFBundleExecutable</key><string>grove</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
