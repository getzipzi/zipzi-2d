# P3F: flat repository layout

The P3F baseline is `b9ef323`. The [file map](p3f-file-map.json) covers all 1,956
tracked entries with original blob/mode, origin, target and merge rationale.
This phase adds ordinary history; the SDK and Editor baseline commits remain
ancestors. P0–P3 evidence is unchanged. Both original checkouts are retained;
no uncommitted Editor benchmark work was imported or modified.

## Result

All 18 owned members are directly under `crates/<package.name>`, including the
Rust comparison runner. `apps/`, `modules/` and the old Rust runner directory are
gone. Editor icons/locales live in `assets/editor`, patches in `vendor`, tools
in `tools`, and documentation in root topic directories. No compatibility
symlinks, wrappers or duplicate source trees were introduced.

`tools/test-sdk.sh` and `tools/test-editor.sh` distinguish entrypoints;
`tools/dev-editor.sh` and `tools/build-editor-app.sh` use root assets and GPL text.
The watcher covers crates, assets, vendor and root Cargo files. Tool tests use
actual temporary flat repositories, caller-relative paths with spaces, custom
Cargo target directories/triples, lifecycle failures and separate checkout PIDs.
Editor Python tool tests are selected explicitly to avoid accidentally discovering
SDK tooling when the directories are merged. Broad Rust/CI package scoping is P4.

The root now contains both license texts and [source-based licensing scopes](../../LICENSING.md).
Merged model inventory and agent guidance retain their original contributions.
Historical archives, completed plans, R0 raw baseline JSON and prior merge evidence
keep original paths and measurements; current navigation explains these exceptions.
Third-party implementation files and runtime assets are byte-identical to the baseline.

Purism moved to `third_party/purism-core` at unchanged
`ad5127666ccffb34552d8e660852995858bee437`. The initially uninitialized checkout
compiled without native validation; initialization at the new path succeeded.
CMake built the native probe and evaluated one synthetic MOC3 sample successfully.
Cargo's build script now registers header/source watches before its missing-header
return, so initializing the submodule after an initial build can trigger rebuilding.
An initialized `cargo check -p kasane-moc3 --locked` passed without the missing-header warning.

## Validation

- `verify-import.py`: exact P1 trees, both histories and original gitlink preserved.
- `verify-flattening.py`: all 1,956 entries accounted for, 18 flat package paths,
  no obsolete directories, executable/symlink modes and immutable artifacts preserved.
- `verify-workspace.py`: default members, Rust 1.95 metadata, editions/licenses,
  pinned GPUI patches/features/profiles and no SDK-to-Editor/GPUI dependency.
  Cargo's member-relative selected-root/default-member fields are normalized when
  comparing root and Editor-crate metadata; the resolved graph must still match.
- `Cargo.lock` is byte-identical to P3F baseline; no dependency upgrade.
- `cargo check --locked -p kasane-sdk -p kasane-python -p cubism-matrix-kasane
  -p kasane-editor -p editor-core -p editor-render --all-targets
  --features kasane-editor/test-support`: passed (uninitialized Purism).
- Separate SDK/Python `cargo check --locked -p kasane-sdk -p kasane-python`: passed.
- Editor core synthetic document/texture import integration: 1 passed before and after native Core initialization.
- Editor `i18n::` tests with `test-support`: 2 passed, including catalog keys/placeholders.
- Editor Python tools: 58/59 initially passed. The watcher test exposed an old
  `dev.sh` invocation; after fixing it, that test passed. SDK inventory tests: 3 passed.
- SDK validator negative controls: 10 passed under Python 3.14; SDK shell entrypoint
  controls: 3 passed. An initial Python 3.9 run failed on the existing `hashlib.file_digest`
  requirement; rerunning with the project's supported interpreter passed.
- Both regenerated Runtime inventories pass `--check`: SDK 954 API entries;
  Editor 726 references in 76 files. Categories and not-connected/not-migrated
  semantics are unchanged; Editor source paths now use the repository root.
- Current README/development/reference/vendor navigation links, shell syntax,
  affected Rust formatting and Git whitespace checks passed.

A real Release smoke run from `/tmp` used the new
`tools/property-chain-benchmark.py` with the original model supplied explicitly:
`--samples 2 --warmup 1 --updates 2 --rounds 1 --filter mesh_name_commit`.
Both plain and instrumented runs completed. `target/p3f-property-smoke` contains
the result and provenance: 1,705 unique source archive members using the new
layout, no local models/cache, and 21 unchanged model file hashes. This confirms
entrypoint and archive behavior, not performance equivalence. Final formatting
only reflowed path expressions after this recorded run.

Both original repository HEADs and working-tree statuses still match the
pre-migration snapshot in the file map. Private execution logs are retained in
`.git/merge-baseline/p3f-*`; local build/benchmark artifacts remain ignored.

Full three-platform CI, complete SDK/Editor suites, wheel/app distribution,
licensed model consolidation and fresh-clone/real-worktree acceptance remain
P4–P6. No GUI, packaging completeness or performance-regression claim is made
from these focused checks. No remote push was performed.
