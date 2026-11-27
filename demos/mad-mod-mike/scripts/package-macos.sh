#!/bin/sh
# Builds dist/Mad Mod Mike.app (universal when the x86_64 target is installed).
set -eu
cd "$(dirname "$0")/.."
app="dist/Mad Mod Mike.app"
bin=target/package/mad-mod-mike
mkdir -p target/package
cargo build --release --bin mad-mod-mike --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release --bin mad-mod-mike --target x86_64-apple-darwin
  lipo -create -output "$bin" "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/mad-mod-mike" \
    "${CARGO_TARGET_DIR:-target}/x86_64-apple-darwin/release/mad-mod-mike"
else
  cp "${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/mad-mod-mike" "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/mad-mod-mike"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Mad Mod Mike</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.mad-mod-mike</string>
  <key>CFBundleExecutable</key><string>mad-mod-mike</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
