# Static project previews

The Editor's GPU Runtime saves a fresh default-pose, fitted model image in the
v1 container. Shell extensions only read that image using `zipzi-project-preview`.
They never start the Editor or evaluate a model. CPU-only saves, empty models,
preview failures and admission limits can omit the image; stale images are not
copied from a previous save.

## macOS

`sh tools/build-editor-app.sh editor debug` builds the Editor plus two sandboxed
app extensions under `Contents/PlugIns`: a Quick Look Thumbnail provider and a
Quick Look data-based Preview provider (macOS 12 or later). The latter returns
the embedded PNG to the system for Space / Finder
preview. Both extensions are signed before the enclosing application. Set
`ZIPZI_CODESIGN_IDENTITY` for a real signing identity; the default is ad hoc.
Distribution still requires the application's final signature and, for a trusted
download, Developer ID provisioning/signing and notarization.

To add extensions to an already-built bundle:

```sh
uv run --locked python tools/packaging/build_project_preview.py --app '/absolute/path/Zipzi Editor.app' --profile debug
codesign --force --sign - '/absolute/path/Zipzi Editor.app'
codesign --verify --deep --strict '/absolute/path/Zipzi Editor.app'
```

Install the application normally. Development registration can use
`pluginkit -a /absolute/path/to/extension.appex`; absolute paths are required.
The System Settings extension preference and a usable application signature
control whether macOS runs an installed extension. An ad hoc build may be
discoverable yet rejected by the system host. Compilation and direct-provider
tests do not certify Finder activation. Do not overwrite the user's file default
or globally reset Quick Look caches as part of testing.

## Windows

Use native x64 Windows, Rust MSVC and the C++ Build Tools with Windows SDK:

```powershell
python tools/packaging/build_project_preview.py --dll target/release/ZipziProjectPreview.dll --test
python tools/packaging/package_windows.py --binary target/release/zipzi-editor.exe --preview-dll target/release/ZipziProjectPreview.dll --output target/releases/Zipzi.zip --revision <full-commit-sha>
```

The portable archive's `Register-FileAssociation.ps1` registers Open With and the
Shell provider for the current user. It also calls `Register-ProjectPreview.ps1`.
Use `-Unregister` before removal or relocation. Unregistration checks the current
DLL path and preserves registrations owned by another location. It does not change
the user's default application. No administrator or `regsvr32` is needed.
Preview handlers are also registered under `SystemFileAssociations\.zipzi` so
Shell discovery works before a default application is selected and after it changes.

After registration, run the generated native test runner against the installed
provider through the system COM host (read-only; it does not register anything):

```powershell
& target/release/project-preview-test/provider-test.exe --registered target/release/project-preview-test
```

This checks isolated activation, thumbnail pixels, preview resizing/unloading and
missing/corrupt image fallback. Direct DLL tests alone do not exercise the system's
registration view. If a development process sees a registration that Explorer does
not, compare with a process launched from Explorer before diagnosing the DLL.
The registered test always checks Shell handler binding and isolated COM activation.
If Windows omits the system thumbnail-cache COM class (as can happen on Server
images), it reports that cache check as unavailable; desktop hosts with the cache
must also pass `IShellItemImageFactory::GetImage`.

The provider implements `IInitializeWithStream`, `IThumbnailProvider` and
`IPreviewHandler` in an Apartment COM server. Explorer uses thumbnail isolation;
Preview runs in the system low integrity preview surrogate. PNG pixels are cached
before closing the input stream. No external model or texture paths are opened.

Native acceptance must verify actual Explorer thumbnails, Preview pane resizing,
transparency, no-preview / corrupt-file fallback and register/unregister after
moving a portable installation. A macOS-only validation run does not cover these.
The `--test` switch runs direct native provider checks: decoded thumbnail pixels,
Preview child-window resize/unload and absent/corrupt input. CI runs it on Windows;
it also covers overlapping previews and repeated DLL load/unload cycles. The provider
only permits DLL unloading after its window class has been unregistered; a surviving
window vetoes unloading. These checks do not replace Explorer interaction acceptance.
On macOS the same switch checks the image reader and deferred PNG reply callback
with native AppKit.
