#!/bin/sh
# Builds dist/Tidepool.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Tidepool.app"
bin=target/package/tidepool
mkdir -p target/package
cargo build --release --bin tidepool --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin tidepool --target x86_64-apple-darwin
  lipo -create -output "$bin" "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/tidepool" \
    "${CARGO_TARGET_DIR:-target}/x86_64-apple-darwin/release/tidepool"
else
  cp "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/tidepool" "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/tidepool"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Tidepool</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.tidepool</string>
  <key>CFBundleExecutable</key><string>tidepool</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
