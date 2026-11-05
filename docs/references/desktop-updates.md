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
Alpha packages without OS developer certificates use `--allow-unsigned` on
both platforms. Windows produces an unsigned installer; Velopack ad hoc signs
the macOS updater and final app. Feed signing remains mandatory in both cases.
This flag is rejected for beta, rc and stable releases.
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
The Alpha workflow supports authenticated updates behind the repository variable
`ZIPZI_DESKTOP_UPDATES=true`. Unset or `false` preserves the previous portable
releases. Feed authentication is required; OS developer certificates are optional.

### GitHub Alpha CI configuration

The required repository configuration is:

| Setting | Kind | Value |
| --- | --- | --- |
| `ZIPZI_DESKTOP_UPDATES` | Variable | `true` |
| `ZIPZI_UPDATE_PUBLIC_KEY` | Variable | Lowercase 64-character hex Ed25519 public key |
| `ZIPZI_UPDATE_SIGNING_KEY` | Secret | Matching 64-character hex private seed |

Without OS credentials, the app jobs produce an ad hoc signed macOS app/DMG and
an unsigned Windows installer, with authenticated feeds and packages. macOS
installation instructions retain the existing unnotarized Alpha notice.
The feed seed is available only to configuration validation and packaging,
never Cargo or shipped artifacts. The pinned packager uses .NET 8 and Velopack
1.2.161, with noninteractive operation and self-update checks disabled.

Optional developer signing can be enabled independently for each OS by providing
that platform's complete set of inputs. A partial set fails preflight:

| Platform | Repository variables | Repository secrets |
| --- | --- | --- |
| macOS | `ZIPZI_MACOS_CODESIGN_IDENTITY`, `ZIPZI_APPLE_TEAM_ID` | `ZIPZI_MACOS_CERTIFICATE_P12` (base64), `ZIPZI_MACOS_CERTIFICATE_PASSWORD`, `ZIPZI_APPLE_ID`, `ZIPZI_APPLE_APP_PASSWORD` |
| Windows | None | `ZIPZI_WINDOWS_CERTIFICATE_PFX` (base64), `ZIPZI_WINDOWS_CERTIFICATE_PASSWORD` |

Configured macOS credentials are imported into a temporary keychain and used for
Developer ID signing and notarization. Windows credentials are imported into
the runner's current-user certificate store for timestamped Authenticode signing.
Cleanup removes imported private keys after the app job.

Enabled builds embed
`https://github.com/<owner>/<repo>/releases/download/desktop-alpha-updates`.
Main pushes and manual runs build and validate artifacts. After a versioned Alpha
release succeeds, a separate job advances the fixed mutable
`desktop-alpha-updates` prerelease. It verifies signatures and local package hashes,
uploads immutable packages first, and then replaces the exact feed/signature bytes.
Numeric version checks prevent older tags from rolling the channel back.
Matching package hashes permit retries after interruption; do not remove packages
that installed clients may still be downloading.

Build reuse binds update enablement, URL, public key and each platform's signing
mode into its manifest identity. Changed configuration forces a rebuild for the
same commit. The downloaded manifest verifies that identity before publication.
Feed publication can be retried independently of the versioned release and PyPI.

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
Alpha builds use ad hoc signing when no developer certificate is configured.
Developer ID signing and notarization remain available for formal distribution.
A writable installation location is needed for replacement. This integration does not
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
