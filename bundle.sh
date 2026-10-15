#!/bin/bash
# Tabboz Simulator - macOS .app bundle builder.
#
# Builds the release binary, generates the .icns from the original
# ZARROSIM.ico (via icon-gen + iconutil) and assembles dist/Tabboz.app.

set -euo pipefail
cd "$(dirname "$0")"

APP=dist/Tabboz.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cargo build --release -p tabboz-app
cargo run --release -p icon-gen --quiet -- "$APP/Contents/Resources/Tabboz.iconset"
iconutil -c icns "$APP/Contents/Resources/Tabboz.iconset" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$APP/Contents/Resources/Tabboz.iconset"

cp target/release/tabboz "$APP/Contents/MacOS/tabboz"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>tabboz</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>info.tabboz.simulator</string>
    <key>CFBundleName</key>
    <string>Tabboz Simulator</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.92q</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

echo "bundle: $APP"
du -sh "$APP"
