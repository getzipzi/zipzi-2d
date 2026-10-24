# P6 local repository acceptance

Local validation on 2026-10-24, starting from P5 commit `0a3628e` on
`codex/merge-editor-monorepo`. This is the local portion of P6. Hosted CI,
official Core comparison and remote cutover require inputs or actions that are
not present in this checkout; they are listed below rather than marked passed.

## Source reconciliation and structure

The original SDK and Editor HEADs still equal the imported baselines
`e27230ab` and `b4e4abc` respectively; there are no later committed changes to
port. The original Editor checkout has another task's uncommitted benchmark
work. It was left untouched and is not represented as merged code.

`verify-import.py`, `verify-flattening.py`, `verify-workspace.py`, both Runtime
inventory `--check` commands and the package-domain check passed. The exact
Editor import, both reachable histories, 18 flat members, unchanged Purism
gitlink, one lockfile and SDK → Editor dependency boundary remain verified.

## Full local gates and focused repair

`./tools/test-sdk.sh full` passed: SDK domain format, complete Rust tests and
the isolated CPU Release wheel gate. `./tools/test-editor.sh full` passed:
Editor domain format, Python tools, all Rust targets, 503 UI tests, real-model
animation/timeline acceptance and native Metal/headless screenshot checks. The
intentional ignored tests remain ignored; these runs do not turn them into
passes.

`./tools/test-sdk.sh wheel --observe` passed after adapting the restored
historical example recipes to the current `crop_targets` API. The observe wheel
passed 72 CPU, 4 inspection and 10 GPU Python tests. Isolated acceptance passed
three GPU frames/crops, the agent repair flow and the pinned independent image
reference. For the reference, the example now renders the imported model's
source canvas explicitly. The pinned image comparison reported zero pixel
difference; the view check still enforces scale and offsets after converting
source-canvas coordinates into SDK model coordinates. Missing attachment
references in this synthetic fixture are discarded only when their exact
expected diagnostics match.

The Purism Core probe passed new-model and imported/edited-model comparison.
The import checker previously compared SDK source pixels with Core runtime
units directly; it now reads each saved stage's `pixels_per_unit` and converts
before applying the existing tolerance. The corrected maximum import error was
0.00037 source pixels. An explicit unit test covers the conversion. The
resulting report is `partial` solely because this run did not provide the
official proprietary Core probe or request GPU together with Purism; the
separate GPU wheel gate passed.

An additional Pyright run over every restored historical example reports
optional-value narrowing and possibly-unbound diagnostics. The configured CI
typing gate targets the CPU recipe, which passed in P5. The examples above ran
successfully through their actual wheel gates; broad example typing cleanup
remains separate from repository migration acceptance.

## Independent checkout and source boundary

A fresh local clone used `git clone --no-hardlinks`, initialized Purism from its
remote, had no Git alternates and contained no `models/local` assets. From its
own empty `target`, `cargo check --locked --offline -p kasane-editor` passed.
An actual second Git worktree at the same commit initialized Purism, resolved
all 18 packages to its own target path, passed domain/workspace checks and 15
dev-tool isolation tests. Its packaging entrypoint tests passed and its own
source archive verified all 2,149 hashed files without private models. The
scripts derive PID, lock and app paths from each checkout; those paths were
covered by the dev-tool tests. No GUI process was launched in either checkout.

From the merged root model directory, a short plain/instrumented Release
property-chain run completed for `mesh_name_commit` with two measured samples,
one warmup, two updates and one round. It retained raw results and provenance
under `target/p6-property-smoke/`; its source snapshot contains 1,714 members
and no local models. This is an entrypoint and provenance check, not evidence of
performance parity or stable latency.

## Remaining external acceptance

- The branch is local and has not been pushed. GitHub license/README display,
  hosted macOS Editor Metal capability and Linux/macOS/Windows SDK CI have not
  been observed.
- The official proprietary Cubism Core probe is not installed here, so the
  dual-Core `--full` wheel gate cannot be claimed. Purism Core and GPU/reference
  checks passed separately.
- The other Editor task's uncommitted work cannot be ported as a stable commit.
  Recheck both original HEADs and port any committed tail work before cutover.
- Final code review, repository hosting and switching new development to this
  checkout are operational decisions; this record does not claim them complete.

Private logs for the broad runs, clone compilation, Purism comparison and
benchmark are retained under `.git/merge-baseline/p6-*`. Runtime outputs stay
under ignored `target/`; neither they nor the licensed models enter Git.
