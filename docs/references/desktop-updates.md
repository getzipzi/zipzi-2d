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
sessions), SDK connections and running jobs. Saved file paths from visible and
resident sessions are passed to the restarted Editor. The updater is armed only
after these checks; its process deadline never includes a save dialog or a running job.

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
entitlements. It explicitly selects the `zipzi-editor` entrypoint and skips
Velopack's PKG installer, packaging the resulting app as the existing DMG.

The output includes a Windows Setup EXE or macOS DMG, `.nupkg` packages,
`releases.<platform>-<architecture>-<channel>.json`, and its `.json.sig` file.
Examples: `windows-x86_64-alpha`, `macos-arm64-alpha`, and `macos-arm64-stable`.
Platform and channel selection are stored by Velopack in the installed package;
one platform never consumes the other's feed. Alpha, beta, rc and stable are
separate channels and never switch implicitly.

`release_downloads.py` accepts the Windows EXE when present and includes signed
update assets in the release's checksums. Existing ZIP releases remain readable
while the production update source and signing credentials are being configured.
The Alpha workflow supports these packages behind the repository variable
`ZIPZI_DESKTOP_UPDATES=true`. Unset or `false` preserves the existing ad hoc Mac
DMG and unsigned Windows ZIP. Enabling updates requires both platforms' signing
inputs and a matching feed key; `prepare` fails before native builds if they are
missing. It never falls back to unsigned updater packages.

### GitHub Alpha CI configuration

Configure these repository **variables**, then use a manual prebuild for the
installed upgrade gate before pushing a release tag:

| Variable | Value |
| --- | --- |
| `ZIPZI_DESKTOP_UPDATES` | `true` to enable signed packages and feed publication; default `false` |
| `ZIPZI_UPDATE_PUBLIC_KEY` | Lowercase 64-character hex Ed25519 public key |
| `ZIPZI_MACOS_CODESIGN_IDENTITY` | Full Developer ID Application certificate identity |
| `ZIPZI_APPLE_TEAM_ID` | Apple Developer team ID |

Configure these repository **secrets**:

| Secret | Value |
| --- | --- |
| `ZIPZI_UPDATE_SIGNING_KEY` | 64-character hex Ed25519 private seed matching the public key |
| `ZIPZI_MACOS_CERTIFICATE_P12` | Base64-encoded Developer ID Application PKCS#12 with private key |
| `ZIPZI_MACOS_CERTIFICATE_PASSWORD` | Password for that PKCS#12 |
| `ZIPZI_APPLE_ID` | Apple ID used for notarization |
| `ZIPZI_APPLE_APP_PASSWORD` | App-specific password for notarization |
| `ZIPZI_WINDOWS_CERTIFICATE_PFX` | Base64-encoded Authenticode PKCS#12 with private key |
| `ZIPZI_WINDOWS_CERTIFICATE_PASSWORD` | Password for that PKCS#12 |

The app jobs install .NET 8 and pinned Velopack 1.2.161. macOS imports its
certificate and notarization profile into a temporary keychain, signs the sandboxed
Quick Look extensions with hardened runtime and timestamps, and notarizes the
packaged app. Windows imports its certificate into the runner's current-user store
and uses timestamped SHA-256 Authenticode signing. Cleanup removes the temporary
keychain or imported Windows private key even after failure. The feed seed is
available only to configuration validation and the packaging step, never Cargo.

Enabled builds embed the fixed source
`https://github.com/<owner>/<repo>/releases/download/desktop-alpha-updates`.
`main` pushes and manual runs only build and validate artifacts. After an Alpha
tag's versioned GitHub release succeeds, `desktop-update-feed` creates or advances
the `desktop-alpha-updates` prerelease. This channel release must remain mutable;
versioned release assets retain their existing no-overwrite policy. All local
feeds, signatures and package hashes are checked before any channel writes.
Packages are uploaded without replacement before the exact feed/signature bytes
are replaced. Existing packages must have matching hashes, so a partial upload can
be retried. The jobs serialize channel publication and compare numeric Alpha
versions, including already-uploaded packages, so older tags cannot roll it back.
An interrupted feed/signature replacement can be repaired by rerunning the same
authenticated build. Do not delete older channel packages that installed clients
may still be downloading.

Build reuse binds the enabled flag, source URL and public key into its manifest
identity. A configuration change forces a new build even for the same source
commit; the downloaded manifest checks that identity again before publication.
Feed publication failure does not rewrite the versioned release or block the
independent PyPI jobs. Rerun only the failed channel job once the cause is fixed.

## Publishing and trust

Publish immutable package files first, then the feed and detached signature.
The signature covers the exact feed bytes. Do not reformat the JSON after
signing. A feed or signature transition can temporarily fail verification and
be retried; it must never cause an unsigned fallback. Keep separate stable
URLs for each channel. The source URL can be shared across platforms because
feed filenames include platform, architecture and channel.

The Editor verifies the feed with the embedded public key before passing its
package filenames, versions and SHA-256 hashes to Velopack. Velopack verifies
the downloaded package's size and hash. Existing cache entries are checked before
reuse; corrupt entries are removed under the updater lock and downloaded again.
The resulting archive is verified before the ready icon appears and checked again
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
uv run --locked python tools/test_tools.py -p test_ci_updates.py
uv run --locked python tools/test_tools.py -p test_publish_updates.py
uv run --locked python tools/test_tools.py -p test_reuse_release.py
uv run --locked python tools/test_tools.py -p test_release_downloads.py
uv run --locked python tools/dev.py test editor ui updates::
```

Local verification on macOS arm64 (2026-11-05): 14 update unit/UI tests and
9 packaging/download tests passed. A fixture app packaged with the actual
Velopack 1.2.161 CLI retained its bundle ID, channel and executable path, included
`UpdateMac`, and produced a package feed that passed signing and verification
with a disposable test key. The fixture used a placeholder executable and did
not exercise Editor upgrades, Developer ID signing, notarization or Quick Look.

The Alpha CI integration additionally passed 35 focused tooling tests, actionlint
and syntax checks for 20 Bash steps. The pinned CLI accepted the noninteractive
packaging arguments on a local fixture. Hosted certificate import, notarization,
Authenticode signing and real GitHub publication still require configured release
credentials and were not exercised by these local checks.

Before enabling a production feed, test two real installed versions A and B:
background check/download, explicit restart, automatic install on quit,
unavailable/corrupt feeds, interrupted downloads, unsaved hidden documents,
active SDK clients/jobs, and uninstall. On Windows keep Explorer's preview pane
open throughout the update and verify both previews and the stable file-open
launcher afterwards. On macOS use formally signed/notarized apps and verify
Quick Look stays enabled after replacement. Windows builds and headless tests
do not substitute for this macOS acceptance gate.
