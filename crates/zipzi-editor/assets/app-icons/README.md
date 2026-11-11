# Application icons

`../branding/zipzi-logo.svg` is the transparent UI mark. The Windows `.ico`
uses the same transparent mark. The macOS `ZipziEditor.icon` is a layered Icon
Composer document with an unmasked white background; `Assets.car` is its
compiled catalog. The `.icns` and its SVG/PNG sources provide the legacy macOS
fallback. Keep these assets in sync when changing the mark.

To recompile the macOS catalog with Xcode's asset catalog tools:

```sh
mkdir -p target/icon-assets
xcrun actool --compile target/icon-assets --platform macosx \
  --minimum-deployment-target 12.0 --app-icon ZipziEditor \
  --output-partial-info-plist target/icon-assets/Info.plist \
  crates/zipzi-editor/assets/app-icons/ZipziEditor.icon
cp target/icon-assets/Assets.car crates/zipzi-editor/assets/app-icons/Assets.car
```

The catalog is checked in so development and packaging do not require the full
Xcode installation. The app bundle includes both the catalog and legacy icon.
