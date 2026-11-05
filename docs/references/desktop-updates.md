# Desktop installation and updates

The Editor uses the same Velopack Rust integration on Windows and macOS. The
application menu and About dialog open Software Update. The title bar has no
update text, progress, animation or error badge: a muted icon appears only after
a package has been authenticated and downloaded. Clicking it opens details;
it never restarts immediately.

Updates are owned by the application, not by an individual window. Automatic
checks run after startup and every four hours. The automatic download/install
preference is saved in the platform configuration directory as `updates.json`.
Automatic installation happens only during a safe application exit. Explicit
restart refuses active operations, unsaved documents (including parked Runtime
sessions), SDK connections and running jobs. Saved open file paths are passed
to the restarted Editor. The updater is armed only after these checks; its
process deadline never includes a save dialog or a running job.

## Release configuration

Install .NET SDK 8 or later on the packaging machine, then install the pinned
CLI (the client machine does not need the SDK):

```powershell
dotnet tool install vpk --version 1.2.161 --tool-path target/tools/vpk
```

Generate an Ed25519 signing seed with the cryptography library in the locked
repository environment. Keep the private seed in the release secret store;
never commit it, put it in command-line arguments or ship it in an application.
Its corresponding raw public key is a 64-character hex string.

Set these environment variables **before building the Editor**:

- `ZIPZI_UPDATE_URL`: an HTTPS directory containing update feeds and packages.
- `ZIPZI_UPDATE_PUBLIC_KEY`: the raw Ed25519 public key in hex.

These values are compiled into the binary. Runtime environment variables and
project files cannot replace the trust key or source. Missing configuration or
a non-Velopack development/portable build offers the release download page
instead of attempting an in-place update.

Use normal locked Cargo builds on Windows and `tools/build-editor-app.sh` on
macOS. For packaging, also set `ZIPZI_UPDATE_SIGNING_KEY` to the private seed
in hex in the packaging process environment. The packaging tool checks that
this matches the public key used for the build.

```powershell
uv run --locked python tools/packaging/package_updates.py --binary target/release/zipzi-editor.exe --preview-dll target/release/ZipziProjectPreview.dll --revision <full-source-sha> --output target/releases --vpk target/tools/vpk/vpk.exe
```

```sh
uv run --locked python tools/packaging/package_updates.py \
  --app 'target/Zipzi Editor.app' --revision <full-source-sha> \
  --output target/releases --vpk target/tools/vpk/vpk
```

Windows signing uses Velopack's `VPK_SIGN_PARAMS` or `VPK_SIGN_TEMPLATE`
configuration. Sign the installer, launcher, updater, EXE and DLL.
Unsigned Windows packages require an explicit `--allow-unsigned` flag and are
only for local validation with a disposable signing seed, never distribution.
For macOS, configure `ZIPZI_CODESIGN_IDENTITY` when building the sandboxed Quick Look
extensions, and `VPK_SIGN_APP_IDENTITY` plus `VPK_NOTARY_PROFILE` when packaging.
Import the matching certificates and notarization credentials on the build
machine first. The packager disables recursive re-signing to retain extension
entitlements. It packages Velopack's resulting app as the existing DMG.

The output includes a Windows Setup EXE or macOS DMG, `.nupkg` packages,
`releases.<platform>-<architecture>-<channel>.json`, and its `.json.sig` file.
Examples: `windows-x86_64-alpha`, `macos-arm64-alpha`, and `macos-arm64-stable`.
Platform and channel selection are stored by Velopack in the installed package;
one platform never consumes the other's feed. Alpha, beta, rc and stable are
separate channels and never switch implicitly.

`release_downloads.py` accepts the Windows EXE when present and includes signed
update assets in the release's checksums. Existing ZIP releases remain readable
while the production update source and signing credentials are being configured.
The current Alpha workflow does not enable a production update source or create
signing credentials automatically; run the updater packaging entrypoint on the
release runners after configuring them, before assembling release assets.

## Publishing and trust

Publish immutable package files first, then the feed and detached signature.
The signature covers the exact feed bytes. Do not reformat the JSON after
signing. A feed or signature transition can temporarily fail verification and
be retried; it must never cause an unsigned fallback. Keep separate stable
URLs for each channel. The source URL can be shared across platforms because
feed filenames include platform, architecture and channel.

The Editor verifies the feed with the embedded public key before passing its
package filenames, versions and SHA-256 hashes to Velopack. Velopack verifies
the downloaded package's size and hash; the cached archive is checked again
immediately before installation. HTML release notes are not rendered.
Do not rotate the pinned key without a separately designed transition release.

Windows keeps the `.zipzi` ProgID and preview CLSID fixed. Open With points at
the stable launcher. Preview registration points at
`<install-root>/shell-preview/<sha256>/ZipziProjectPreview.dll`, outside the
replaceable `current` directory. Already loaded copies stay available; only
released copies are removed opportunistically. Upgrade does not unregister the
previous handler first. Uninstall removes registration only when it still owns
the recorded paths. Registration never writes the user's default-app choice.

macOS retains `dev.zipzi.editor` and both embedded Quick Look extensions.
Developer ID signing, notarization and a writable installation location are
required for the intended distribution experience. This integration does not
promise that an OS authorization can never recur, or implement automatic
rollback after an otherwise successful update that later crashes.

## Validation

```powershell
uv run --locked python tools/test_tools.py -p test_package_updates.py
uv run --locked python tools/test_tools.py -p test_release_downloads.py
uv run --locked python tools/dev.py test editor ui updates
```

Before enabling a production feed, test two real installed versions A and B:
background check/download, explicit restart, automatic install on quit,
unavailable/corrupt feeds, interrupted downloads, unsaved hidden documents,
active SDK clients/jobs, and uninstall. On Windows keep Explorer's preview pane
open throughout the update and verify both previews and the stable file-open
launcher afterwards. On macOS use formally signed/notarized apps and verify
Quick Look stays enabled after replacement. Windows builds and headless tests
do not substitute for this macOS acceptance gate.
