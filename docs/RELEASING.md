# Releases

The first release is `v0.1.0-alpha.1`, an Apple Silicon macOS Editor Alpha.
PyPI and crates.io publication is prepared separately and is not triggered by
the Mac release workflow. SDK CI and Editor CI remain manual-only.

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

Prepare the next version from the repository root:

```sh
python3 tools/packaging/release.py set 0.1.0-alpha.2
cargo metadata --locked --offline --no-deps --format-version 1 > /dev/null
python3 tools/packaging/release.py check --tag v0.1.0-alpha.2
```

The tool also supports stable, `beta.N` and `rc.N` versions. Only Alpha tags
currently trigger Mac publication. Project-file schemas, native ABI contracts
and IPC protocol versions are separate compatibility contracts; do not change
them merely to bump a release version.

## Mac Alpha publication

1. Review and commit the intended source and release changes. A tag identifies
   committed files; uncommitted changes are not included.
2. Add `docs/releases/<tag>.md` with release notes, supported architecture and
   known limitations. Run the focused tooling checks below and appropriate
   native acceptance locally for the features being released.
3. Push the source and an annotated Alpha tag:

   ```sh
   git tag -a v0.1.0-alpha.1 -m 'Zipzi Editor 0.1.0 Alpha 1'
   git push origin main refs/tags/v0.1.0-alpha.1
   ```

4. Follow **Actions → Package macOS App**. The tag must exactly match the
   workspace version. One `macos-15` runner builds the Release app, validates
   its contents, ad hoc signs it, verifies the signature, and creates a zip.
   Corresponding complete workspace source and SHA-256 checksums accompany it.
5. A small Linux job verifies downloaded checksums and creates a GitHub
   **prerelease**, without marking it Latest. Release assets remain available
   after the three-day Actions artifacts expire. Repository access rules apply
   to private-repository Releases.

Ordinary commits and PR updates run nothing automatically. A manual Package
macOS App run produces temporary artifacts without creating a GitHub Release.
No Apple certificate is required for this Alpha; it is not Developer ID signed
or notarized. Follow the release notes for first-launch instructions. Published
tags and assets should not be replaced: fix the issue and use the next Alpha
number. A failed run that created no Release can be retried from Actions.

The first Alpha includes the Mac app only. Source archives include the required
workspace layout, vendored patches, initialized Purism sources and notices;
proprietary Cubism SDK/Core binaries and local models are excluded. SDK remains
MIT; Editor remains GPL-3.0-only. Packaging success is not a substitute for native
UI/GPU acceptance.

## Python package preparation

The public client distribution is **`zipzi`**, imported as `zipzi`. The optional
native backend is **`zipzi-native`**, imported as `zipzi_native`. Their sources
remain at `crates/zipzi-python2/client` and `crates/zipzi-python2`; the Rust adapter
crate retains its internal `zipzi-python2` name. The old `crates/zipzi-python`
adapter is not a public release target.

Run **Actions → Prepare Python Packages → Run workflow** to produce source and
wheel archives and installed Local/Connected acceptance evidence. This is
manual-only and contains no registry upload step or registry credentials.
Equivalent local preparation:

```sh
uv sync --locked
python3 tools/packaging/release.py check
uv run --locked python crates/zipzi-python2/tools/distribution.py \
  --output target/python-release-0.1.0a1
```

The native backend version comes from Cargo via Maturin and is mapped to Python
version syntax; the client and exact native-to-client requirement are checked by
the release tool. Current Python support is CPython 3.14, with native source
acceptance on macOS/Unix. Expand the wheel matrix only when those platforms are
actually supported and tested.

Before a future public Python release, configure PyPI/TestPyPI project ownership
and Trusted Publishing for a dedicated upload workflow. Exercise TestPyPI first,
then upload the already-tested immutable artifacts to PyPI. This repository does
not yet enable either upload; creating a Mac Alpha tag publishes neither package.

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
