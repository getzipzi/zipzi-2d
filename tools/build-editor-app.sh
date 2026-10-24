#!/bin/sh
set -eu
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/." && pwd)
. "$tools_dir/workspace/workspace.sh"
case "${1:-editor}" in
  editor)
    package="zipzi-editor"
    name="Zipzi Editor"
    bundle_id="dev.zipzi.editor"
    icon_asset="$repo_root/crates/zipzi-editor/assets/app-icons/zipzi-editor-icon-vector.icns"
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
binary=$(python3 "$repo_root/tools/workspace/workspace_tools.py" executable "$artifact_log")
app="$PWD/target/$name.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/$package"
cp "$repo_root/LICENSE-GPL-3.0" "$app/Contents/Resources/LICENSE"
for notice in LICENSE-MIT LICENSE-GPL-3.0 LICENSING.md THIRD_PARTY_NOTICES.md; do
  cp "$repo_root/$notice" "$app/Contents/Resources/$notice"
done
mkdir -p "$app/Contents/Resources/vendor-notices"
find "$repo_root/vendor" -type f \( -iname 'license*' -o -iname 'notice*' -o -iname 'copying*' \) | while IFS= read -r notice; do
  relative=${notice#"$repo_root/vendor/"}
  mkdir -p "$app/Contents/Resources/vendor-notices/$(dirname "$relative")"
  cp "$notice" "$app/Contents/Resources/vendor-notices/$relative"
done
cp "$repo_root/vendor/README.md" "$app/Contents/Resources/vendor-notices/README.md"
cp "$icon_asset" "$app/Contents/Resources/AppIcon.icns"
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
