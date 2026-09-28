#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
case "${1:-editor}" in
  editor)
    package="kasane-editor"
    name="Kasane Editor"
    bundle_id="dev.kasane.editor"
    ;;
  playground)
    package="component-playground"
    name="Kasane Component Playground"
    bundle_id="dev.kasane.component-playground"
    ;;
  *)
    printf 'usage: %s [editor|playground] [release|debug]\n' "$0" >&2
    exit 2
    ;;
esac
profile="${2:-release}"
case "$profile" in
  release) cargo_profile="release" ;;
  debug) cargo_profile="dev" ;;
  *)
    printf 'usage: %s [editor|playground] [release|debug]\n' "$0" >&2
    exit 2
    ;;
esac
cargo build --locked --profile "$cargo_profile" -p "$package"
binary="target/$profile/$package"
app="$PWD/target/$name.app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/$package"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundleExecutable</key><string>$package</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$app"
