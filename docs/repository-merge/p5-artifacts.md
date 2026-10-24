# P5: local models and distribution artifacts

Completed locally on 2026-10-24 on `codex/merge-editor-monorepo`. Commands and
paths below refer to the repository root. P5 validates the assembled local
artifacts; P6 still owns broad regression, fresh-clone/worktree isolation and
hosted CI.

## Models

The two original `models/local` trees were hashed before consolidation: the
SDK had 27 files and the Editor had 51. Shared paths had no different bytes.
The root tree contains the union of 48 model assets; `.DS_Store`, `.kasane.lock`
and `cat/cat_import.af~lock~` were excluded as machine/application state.
No source model asset was overwritten or removed. The original Editor lock
changed independently during verification, so it is excluded from the asset
comparison. The private before/after hashes and provenance remain in
`.git/merge-baseline/p5-models.json`; restricted assets and hashes are not in Git
or source archives. All 26 referenced model3 files resolve from the root model
tree, including textures and related project files.

Two real-model Editor acceptance filters passed: `real_cubism_editor_mao_imports_from_both_entries_and_saves`
and `local_v3a9_cubism_export_retains_expression_poses_and_playback_frames`.
They exercise imports, save/export and animation with the relocated assets.

## Source, wheel and application

`cargo package --list -p kasane-editor` showed that a standalone crate package
omits root `assets/editor` and `vendor` inputs used at build time. The three
Editor crates now declare `publish = false`. The distributable Editor source is
the complete workspace archive produced by `tools/package_source.py`, with
per-file SHA-256 hashes and checkout status in `SOURCE-MANIFEST.json`.
`tools/verify_artifacts.py --source` passed; extraction followed by
`cargo metadata --locked --offline --no-deps` found all 18 workspace members,
root Editor resources and Purism sources. The archive excludes local models,
build output, caches and machine Cargo configuration. Packaging rejects a
missing Purism checkout, escaping symlinks and tracked private/generated files.

A real CPython 3.14 macOS arm64 Release wheel was built and inspected. It has
the native extension, typing marker/stub, MIT license and third-party notices;
the notice copies match the canonical root files byte-for-byte. The wheel was
installed in an isolated environment. `tools/validate_sdk.py --wheel` passed
its CPU checks (72 tests plus creation/export and import/edit), reporting
`partial` because optional GPU and dual-Core checks were not run. The restored
SDK example recipes had been deleted in the imported SDK baseline even though
the validator and CI still referenced them. Their synthetic fixture and current
API calls are now restored; the import/edit recipe explicitly resolves its two
expected missing-attachment diagnostics before export. Installed-wheel Pyright
reported zero errors for the CPU recipe, and that recipe ran from outside the
checkout. The ten validator unit tests passed.

Real debug and release macOS Editor app bundles were built and separately
retained under `target/p5-apps/`. The artifact verifier passed for both: the
Cargo-reported executable is in `Contents/MacOS`, and the bundle has the icon,
MIT/GPL texts, licensing guidance, third-party notices and vendor license
files. Both SVG icons and both locale descriptions were found embedded in each
binary. This validates packaging, not a GUI launch or notarization. Packaging
tool tests passed (six focused cases).

The distribution commands and interpretation of the wheel's `partial` result
are documented in [the development guide](../development/distribution.md).
The SDK CI wheel job now checks notice consistency before building and checks
the built wheel contents. Root ignore rules cover the new native-extension,
output/cache, restricted-model and machine-config paths; no such files are
tracked.

The branch has not been pushed, so GitHub's online license recognition and
README presentation remain unverified. GPU/dual-Core wheel acceptance, full
regression, hosted CI and independent checkout tests are P6 work.
