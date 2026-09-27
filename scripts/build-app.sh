#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build -p kasane-editor
app="$PWD/target/Kasane Editor.app"
mkdir -p "$app/Contents/MacOS"
cp target/debug/kasane-editor "$app/Contents/MacOS/kasane-editor"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.kasane.editor</string>
<key>CFBundleName</key><string>Kasane Editor</string>
<key>CFBundleExecutable</key><string>kasane-editor</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$app"
