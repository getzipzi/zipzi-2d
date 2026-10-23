#!/bin/sh
set -eu
. "$(dirname "$0")/workspace.sh"
case "${1:-editor}" in
  editor)
    package="kasane-editor"
    name="Kasane Editor"
    bundle_id="dev.kasane.editor"
    icon_asset="$editor_root/assets/kasane-editor-icon-vector.icns"
    ;;
  *)
    printf 'usage: %s [editor] [release|debug]\n' "$0" >&2
    exit 2
    ;;
esac
profile="${2:-release}"
case "$profile" in
  release) cargo_profile="release" ;;
  debug) cargo_profile="dev" ;;
  *)
    printf 'usage: %s [editor] [release|debug]\n' "$0" >&2
    exit 2
    ;;
esac
artifact_log=$(mktemp)
trap 'rm -f "$artifact_log"' 0
cargo build --locked --message-format=json-render-diagnostics --profile "$cargo_profile" -p "$package" > "$artifact_log"
binary=$(python3 "$editor_root/scripts/workspace_tools.py" executable "$artifact_log")
app="$PWD/target/$name.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/$package"
cp "$editor_root/LICENSE" "$app/Contents/Resources/LICENSE"
if [ -f "$icon_asset" ]; then
  cp "$icon_asset" "$app/Contents/Resources/AppIcon.icns"
fi
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$bundle_id</string>
<key>CFBundleName</key><string>$name</string>
<key>CFBundleExecutable</key><string>$package</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$app"
