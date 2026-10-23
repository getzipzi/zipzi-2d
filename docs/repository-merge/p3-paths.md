# P3: paths, development entrypoints and benchmark provenance

## Changes

Editor shell entrypoints share a small path setup in `apps/editor/scripts/workspace.sh`:
application assets/licenses come from `apps/editor`, Cargo runs at the repository
root, and relative model/output arguments and `CARGO_TARGET_DIR` are anchored to
the caller before changing directory. Defaults for models and outputs use root
`models/local` and `target`. The SDK's existing Python/CMake root layout is unchanged.

Development builds remain isolated in root `target/dev`, deliberately overriding
inherited `CARGO_TARGET_DIR`. PID records, readiness records, locks, logs, dev
session state and the dev bundle remain under that checkout's root `target`.
Watch mode observes Editor, SDK modules and the root manifest/lock, including
`.lock` changes. Ownership checks, launch recovery, restart gaps and stopping the
watcher on app exit are preserved.

Build-app, dev reload and window benchmark now locate binaries from Cargo's
`compiler-artifact` messages rather than guessing a profile directory. This
supports custom output directories and configured target triples. App packaging
uses the Editor GPL license and Editor icon. Bundle locations are still root
`target`, independently of where Cargo emitted the binary.

Rust model and output discovery moves up to the workspace root; shared fixtures
now reference root `tests/fixtures` and `modules/*/tests/fixtures`. Crate-internal
`include_str!`, `include_bytes!` and `#[path]` imports keep their original layout.
The existing optional-model skip versus explicit-missing-model failure rules
are unchanged. No local model or machine Cargo configuration was copied.

Property-chain provenance schema 2 contains a single repository HEAD/dirty
record, root lock hash, compiler/build environment, model hashes and repo-relative
source hashes. It writes one `repository.diff` and one source archive. The explicit
source list covers root Cargo/Python configuration, modules, shared tests/tools,
the Rust benchmark member, Editor crates/scripts/assets/vendor. Git ignore rules
exclude untracked caches/extensions; models/local is outside the source list;
external symlinks are rejected. Gitlink identity is recorded separately. Existing
historical benchmark JSON and hashes were not rewritten.

Window and Timeline scripts also record single-repository provenance, compiler,
root lock, source and model hashes. Their model path is explicit so unrelated
inherited benchmark variables cannot change provenance. Trace/report scripts are
resolved from the application directory. Tool test discovery uses that directory
while Cargo executes in the root. Package-set changes remain P4 work.

## Runtime inventory baseline issue

Both inventory `--check` commands already failed in the original P0 source
checkouts before regeneration. This was reproduced read-only against their
unchanged HEADs; it is not a migration regression. The original generators were
run against the imported sources without changing classification rules or marking
Runtime work complete:

- Editor: 692 → 726 references across 76 files, retaining `not-migrated` status.
- SDK: 952 → 954 APIs; existing `not-connected` statuses remain.

Both regenerated indexes pass `--check`. Editor index paths remain relative to
`apps/editor`; SDK index paths remain relative to the repository root. Current
Runtime navigation links were updated; old R0 evidence retains original paths
and SHAs.

## Validation

- Editor `cargo check --locked -p kasane-editor -p editor-core -p editor-render
  --all-targets --features kasane-editor/test-support`: passed, including examples
  and native test target compilation.
- Three focused editor-core integration tests passed: SDK model3/PSD import and
  texture round trip; shared asymmetric texture preview; animation fixture
  reference navigation. Each command executed one selected test, not the full suite.
- Real absolute `apps/editor/scripts/test.sh tools` invocation from `/tmp`: 59
  Python tests passed using the normal `python3` entrypoint. Coverage includes
  failure propagation, filter forwarding, watcher lifecycle/recovery, custom
  target/triple, root/app/unrelated caller directories, spaces in paths, GPL/icon
  packaging, concurrent checkout ownership, bounded source archives, all four
  shell benchmark wrappers and single-repository provenance.
- Packaging and window launch tests use fake Cargo/host tools and stub processes;
  they validate paths/ownership/failures, not native GUI behavior or real release
  app packaging. Compiler checks above use the actual source graph.
- SDK inventory's 3 Python tests passed; both inventory checks passed; shell
  syntax, package-scoped Rust formatting and `git diff --check` passed.

The first two synthetic property short-run attempts built both Release binaries
and wrote provenance, but stopped before timing because other Cargo/Rust compiler
processes were active. A later synthetic run reached the probe and failed with
`no editable mesh at default pose`: `external_v50` is a format fixture, not a
suitable property-chain workload. No workload assertions or concurrency guards
were removed. A Mao attempt also hit the concurrency guard. All diagnostics and
partial outputs remain local under `.git/merge-baseline/p3-*` and `target/p3-*`.

The final short run uses the original checkout's Mao model through an explicit
absolute input argument. Code, dependencies, lockfile, artifacts and source archive
come only from zipzi-2d. The asset stays in its original location; consolidation
is P5 work. The final run passed from `/tmp` after waiting for a quiet compiler
window: one plain/instrumented pair, 2 samples, 1 warmup, 2 updates, filtered to
`mesh_name_commit`. Both runs validated commit/preview/history behavior and the
instrumented accounting. Model and source hashes stayed unchanged throughout.
The archive has 1705 unique repo-relative files matching the provenance manifest,
with no local model, target tree or sibling checkout included. Results are at
`target/p3-property-smoke-mao-2/summary.md`. This is an entrypoint check, not a
performance comparison.

## Remaining scope

P4 must replace broad `--workspace` package selection and split CI. P5 still owns
asset consolidation, complete license/artifact metadata and the architecture map.
P6 still owns real debug/release app bundles, native development reload, licensed
asset acceptance, full domain regression, wheel acceptance, fresh clone and real
second-worktree validation. Temporary two-checkout fixtures here do not substitute
for that final acceptance.

Original SDK and Editor HEADs remain the frozen P0 SHAs. At the end of this stage,
the original Editor checkout has another task's uncommitted UI benchmark changes;
these were neither imported nor edited. Reconcile them after their task commits
before the eventual repository switch. The original SDK checkout remains clean.
