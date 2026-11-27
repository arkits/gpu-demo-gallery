#!/bin/sh
# Builds dist/Subsurface Scattering.app (universal when the x86_64 target is installed).
# The app asks for Sushi.ati, or finds it beside the bundle.
set -eu
cd "$(dirname "$0")/.."
app="dist/Subsurface Scattering.app"
bin=target/package/subsurface-scattering
mkdir -p target/package
cargo build --release -p subsurface-scattering --target aarch64-apple-darwin
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --release -p subsurface-scattering --target x86_64-apple-darwin
  lipo -create -output "$bin" \
    target/aarch64-apple-darwin/release/subsurface-scattering target/x86_64-apple-darwin/release/subsurface-scattering
else
  cp target/aarch64-apple-darwin/release/subsurface-scattering "$bin"
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$bin" "$app/Contents/MacOS/subsurface-scattering"
cat > "$app/Contents/Info.plist" <<'P'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Subsurface Scattering</string>
  <key>CFBundleIdentifier</key><string>io.github.arkits.subsurface-scattering</string>
  <key>CFBundleExecutable</key><string>subsurface-scattering</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
P
codesign --force --sign - "$app"
echo "$app"
