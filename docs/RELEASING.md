# Releases

The first release, `v0.1.0-alpha.1`, contains the Apple Silicon macOS Editor.
Subsequent Alpha releases use one workflow for the macOS and Windows Editors
and the Python packages. Rust crates.io publication remains separate. SDK CI
and Editor CI remain manual-only.

## Version identity

`[workspace.package].version` in the root `Cargo.toml` is the release source of
truth. All owned Rust packages use that version. The release tool synchronizes
exact versions on internal path dependencies, workspace entries in `Cargo.lock`,
the Python client version and its native dependency requirement.

| Target | Example |
| --- | --- |
| Rust workspace | `0.1.0-alpha.1` |
| Git tag / GitHub prerelease | `v0.1.0-alpha.1` |
| Python `zipzi` / `zipzi-native` | `0.1.0a1` |
| App `CFBundleShortVersionString` | `0.1.0` |
| App `CFBundleVersion` | numeric workflow run number and attempt |
| App `ZipziReleaseVersion` | `0.1.0-alpha.1` |
| App `ZipziSourceRevision` | source commit SHA |

Prepare the next version from the repository root. Regenerate the Python SDK
catalog and reference so their embedded package version is synchronized:

```sh
python3 tools/packaging/release.py set 0.1.0-alpha.2
uv run --locked python crates/zipzi-python2/tools/generate.py
cargo metadata --locked --offline --no-deps --format-version 1 > /dev/null
python3 tools/packaging/release.py check --tag v0.1.0-alpha.2
```

The tool also supports stable, `beta.N` and `rc.N` versions. Only Alpha tags
currently trigger Alpha publication. Project-file schemas, native ABI contracts
and IPC protocol versions are separate compatibility contracts; do not change
them merely to bump a release version.

## Alpha publication

The **Alpha release** workflow (`.github/workflows/release-alpha.yml`) builds
four independent jobs: the macOS Apple Silicon app and source archive, the
Windows x86-64 portable app, and Python packages on macOS and Windows. Python
builds rebuild wheels from their source archives and run isolated installed
Local/Connected acceptance. A collection job checks both acceptance reports,
commit identity, wheel tags and file hashes. It selects the macOS copies of the
source archives and pure wheel and one native wheel per platform, then checks
the distribution metadata. Only after all builds and collection succeed does a tag run create the GitHub prerelease
and then publish the tested Python distributions to PyPI.

Before the first PyPI publication:

1. Configure the GitHub Actions repository secret `PYPI_API_TOKEN` with an
   account-scoped PyPI token able to create and publish both `zipzi` and
   `zipzi-native`. Verify the account email and confirm both project names are
   available or owned by that account. The workflow uses the secret in its two
   publishing jobs, with environments `pypi-zipzi` and `pypi-zipzi-native`.
   Token-based publishing does not produce PyPI Trusted Publisher attestations.
2. Configure the two GitHub environments and their deployment protections as
   appropriate. To rotate the token, replace the existing Actions secret; never
   add a token to the repository or logs.
3. Verify Windows native desktop/GPU acceptance on a real Windows machine;
   hosted build and Python checks do not validate the complete UI experience.
   Also run the appropriate local macOS acceptance and release tooling checks.

For each release:

1. Set and check the version, then review and commit all source changes.
2. Push the workflow changes to the default branch. Run **Actions → Alpha
   release → Run workflow** on the intended commit as a rehearsal. A manual run
   builds and uploads seven-day Actions artifacts only; it never publishes.
3. Inspect the completed artifacts and acceptance reports, then push an annotated
   Alpha tag matching the workspace version. For example:

   ```sh
   git tag -a v0.1.0-alpha.2 -m 'Zipzi 0.1.0 Alpha 2'
   git push origin main refs/tags/v0.1.0-alpha.2
   ```

4. Follow **Actions → Alpha release**. Once all builds pass, the workflow creates
   a GitHub prerelease containing the Mac app ZIP and DMG, Windows portable ZIP,
   complete workspace source, Python distributions and `SHA256SUMS`. GitHub
   automatically generates the release notes. Separate PyPI jobs then publish `zipzi` followed by `zipzi-native` using the
   configured API token.

The Windows ZIP contains the executable, licenses/notices and build identity;
it is unsigned. The macOS app is ad hoc signed, without Developer ID signing
or notarization. macOS may block the first launch; if you trust the build, open
System Settings → Privacy & Security → Open Anyway after attempting to launch it.
Actions artifacts expire after seven days; GitHub Release assets persist.
Python targets CPython 3.14; native wheels cover macOS arm64 and Windows x86-64.
Other platforms can use the pure client; native source builds are not covered by
this matrix. No Rust crates are published by this workflow.

The two external services cannot publish atomically. If GitHub succeeds and PyPI
fails, the GitHub prerelease remains; inspect the failed PyPI job and fix the
token or account configuration before rerunning the failed job. If the client job
succeeded but the native job failed, the client is already public. PyPI uploads individual
files and may partially succeed: check both PyPI projects and all files before
retrying. Already-published files cannot be replaced; use a new Alpha version
if the release needs changed artifacts. Do not blindly rerun the entire workflow
against an existing GitHub release or tag. GitHub Release assets and PyPI
versions must not be overwritten.

Development gates run for relevant pull requests; SDK CI and Editor CI remain
manual. The Alpha workflow runs only on a matching tag push or manual dispatch.
Packaging success does not establish native UI/GPU or licensed-model acceptance.

## Python package identities

The public client distribution is **`zipzi`**, imported as `zipzi`. The optional
native backend is **`zipzi-native`**, imported as `zipzi_native`. Their sources
remain at `crates/zipzi-python2/client` and `crates/zipzi-python2`; the Rust
adapter retains its internal `zipzi-python2` name. Equivalent local preparation:

```sh
uv sync --locked
python3 tools/packaging/release.py check
uv run --locked python crates/zipzi-python2/tools/distribution.py \
  --output target/python-release-0.1.0a2
```

## Rust package preparation

List the SDK crates in dependency order:

```sh
python3 tools/packaging/release.py cargo-plan
cargo package --locked -p zipzi-api-derive --list
cargo publish --locked -p zipzi-api-derive --dry-run
```

The plan excludes Editor crates, the benchmark executable and Python adapters.
Published dependencies use the exact synchronized Alpha version. Publish lower
dependencies before their consumers when a future crates.io release is approved.
Before first publication, higher crates cannot complete registry-based packaging
verification until their dependencies exist on the registry. Do not bypass that
verification with `--no-verify`.

Test each packaged crate independently from an extracted `.crate` archive.
In particular, `zipzi-moc3` uses optional Purism sources outside its crate: its
published default-feature behavior must be explicitly validated or made
self-contained before public Rust publication. Review crate metadata, packaged
licenses and resource inclusion, then configure crates.io ownership and publishing
credentials. No current workflow runs `cargo publish`.

## Focused release controls

```sh
uv run --locked python tools/test_tools.py -p test_release.py -v
uv run --locked python tools/test_tools.py -p test_workspace_entrypoints.py -v
uv run --locked python tools/test_tools.py -p test_python2_tools.py -v
uv run --locked python tools/test_tools.py -p test_python2_distribution.py -v
uv run --locked python tools/test_tools.py -p test_package_source.py -v
```

These controls validate version translation, tag rejection, dependency order,
bundle metadata, path-independent packaging and source boundaries without
compiling the entire SDK or Editor.
