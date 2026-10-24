# Repository merge execution record

## Scope and status — 2026-10-24

The user requested a new local repository named `zipzi-2d`, with `kasane-2d`
as upstream, and to begin the repository merge plan. This checkout is an
independent clone (no shared object alternates or hardlinks), on
`codex/merge-editor-monorepo`. `upstream` uses the existing kasane-2d GitHub URL.
No new GitHub repository, push, remote rename, or archive was performed.

P0 baseline capture, P1 history import, P2 workspace integration and P3 path/tool
adaptation are complete.
**Revised scope:** the user now requests a fully flattened repository. P3F
(flatten all owned crates, consolidate assets/tools/docs/vendor, then adapt paths)
is complete; P4 test/CI routing, P5 artifact/model checks and P6 local
acceptance have also passed. P0–P3 results describe the intermediate layout,
not acceptance of the flattened layout; see the P3F evidence. Cargo resolves
one workspace and lockfile; Editor entrypoints and fixtures use this checkout.
Both domains' full local gates, GPU wheel/reference, Purism parity, isolated
checkout and focused benchmark ran. Hosted CI and final cutover remain open.

The governing [implementation plan](../plans/repository-merge-plan.md)
retains the design and acceptance matrix. Its original snapshot is historical;
this record and baseline.json identify the actual imported sources. The user’s
new-checkout request replaces the plan’s suggestion to work inside kasane-2d.

## P0: frozen sources and recovery

- SDK: `e27230ab2fed1df4b4d0743d28fb1ae3fca3405e` (15 workspace packages).
- Editor: `b4e4abc737f88bdb895dfec960444dd3183369b7` (3 workspace packages).
- Both source main worktrees were clean when captured. Existing worktrees were
  inventoried without modification. An active UI benchmark task was observed;
  its outputs and processes were not used or changed.
- `baseline.json` records lockfile hashes, package lists, local configuration
  hashes and model counts. `*-tree.txt` records every tracked path, mode and
  object ID, including vendor patches and the original configuration sources.
- Local recovery refs: `refs/merge-baseline/sdk`, `refs/merge-baseline/editor`.
- Local complete Git backup: `.git/merge-baseline/sources.bundle`.
  `git bundle verify .git/merge-baseline/sources.bundle` passed.
- `.git/merge-baseline/` also holds original lockfiles/manifests, no-dependency
  Cargo metadata, worktree/status inventories and per-file model SHA-256 lists.
  These local evidence files are intentionally not committed or copied into
  source archives. Preserve this directory when moving this checkout.
- At P0, SDK had 27 local model files and Editor had 51, with no shared-path
  content conflicts. P5 later consolidated their assets; see the P5 record.
- Both original `cargo metadata --locked --no-deps --format-version 1` commands
  passed. This verifies manifest/package metadata, not compilation or full
  dependency resolution. No expensive baseline suite was repeated, and no
  prior benchmark/test outcome is asserted here.
- Purism remains the uninitialized gitlink
  `ad5127666ccffb34552d8e660852995858bee437`.

To inspect the backup, use `git bundle list-heads .git/merge-baseline/sources.bundle`.
It contains both baseline histories, not ignored models or machine configuration.
The source checkouts remain available and unchanged.

## P1: exact, history-preserving import

Baseline record commit: `28b72a8a`.
Import commit: `0284d87e5c066c4f144c90d906fccf6ed00692db`.
The import uses `git subtree add --prefix=apps/editor` without squash; its second
parent is the exact Editor baseline. Original SDK and Editor commits remain
reachable, with no rewritten objects. No imported Editor file was edited in
that commit.

Run the repeatable static acceptance check from any directory:

```sh
python3 /path/to/zipzi-2d/docs/repository-merge/verify-import.py
```

Passed: all 1,113 Editor entries match their original relative paths, blob IDs
and modes; the SDK tree is unchanged by the import; both baseline SHAs are
ancestors; the Purism gitlink is unchanged. Tree comparison includes executable
bits and any symlink entries. Only tracked source was imported.

To inspect Editor history before the prefix was introduced:

```sh
git log b4e4abc737f88bdb895dfec960444dd3183369b7 -- crates/editor/Cargo.toml
git show b4e4abc737f88bdb895dfec960444dd3183369b7:crates/editor/Cargo.toml
```

Use paths relative to the original Editor root at that revision. Do not rely
on `git log --follow` automatically crossing the subtree merge.

## P2: unified Cargo workspace

See [P2 validation and decisions](p2-workspace.md) and the
[dependency audit](p2-dependency-audit.json). Re-run structural acceptance with
Python 3.11+:

```sh
python3.14 docs/repository-merge/verify-workspace.py
```

## P3: paths and tooling

See [P3 implementation and validation](p3-paths.md).

## P3F: flat layout and integrated resources

See [P3F validation](p3f-flattening.md), the [complete file map](p3f-file-map.json)
and `verify-flattening.py`. All 18 owned crates now live under root `crates/`;
Editor assets, tools, documentation and vendor patches use root directories.

## P4: test domains and CI

See [P4 implementation and local validation](p4-test-domains.md). SDK and Editor
have separate complete gates with exhaustive package coverage checks.

## P5: models and distribution artifacts

See [P5 implementation and local validation](p5-artifacts.md). The local model
assets are consolidated under the ignored root tree. A workspace source archive,
real Release wheel and debug/release macOS app bundles were built and checked.

## P6: local repository acceptance

See [P6 local results and remaining external gates](p6-local-acceptance.md).
Both full local domains, GPU wheel/reference, Purism comparison, fresh clone,
second worktree and a short integrated-model benchmark passed. Hosted and
cross-platform CI, official proprietary Core, GitHub license recognition and
final development cutover remain unverified. Recheck original HEADs before
cutover and reconcile any new committed work.
