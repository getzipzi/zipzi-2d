# Development tools

Run commands from the repository root, using `uv run --locked python` for Python
tools. Implementations live here; their regression tests live in
[`tools/tests/`](tests/README.md). Tooling tests use automatic discovery:

```sh
uv run --locked python tools/test_tools.py
uv run --locked python tools/test_tools.py -p test_cargo_domain.py -v
```

## Directory ownership

The unified cross-platform test entrypoint is `dev.py`. Shell test entrypoints are thin wrappers. Other daily entrypoints include: `test-sdk.sh`, `test-editor.sh`,
`test_tools.py`, `dev-editor.sh`, `build-editor-app.sh` and `dev-python`.
Their implementation helpers and specialized commands live under the directories
below. Paths in this guide are relative to `tools/` unless a command starts with `tools/`.

| Directory | Responsibility |
| --- | --- |
| `workspace/` | Cargo package domains, shared shell paths, artifact discovery and provenance |
| `python/` | Python2 contract, comparison and agent test helpers |
| `packaging/` | Workspace source archives and source/wheel/app verification |
| `benchmarks/` | Native Editor benchmarks, offscreen measurement and trace reports |
| `validation/` | SDK acceptance, image comparisons, model/runtime probes and Purism checks |
| `validation/probes/`, `validation/cmo3/` | C++ probe builds and Cubism Java inspection helper |
| `fixtures/` | Shared model/animation fixture generators; generated inputs live in root `fixtures/` |
| `runtime/` | Runtime inventories, ownership guards, CPU/native paired benchmarks |
| `tests/` | Regression tests for all tooling groups |

Add specialized scripts to their owning directory and update this guide and
callers. Python scripts may import sibling modules; cross-group dependencies
should stay explicit. Shell callers set `tools_dir` before sourcing
`workspace/workspace.sh`, so nested benchmark scripts preserve caller-relative
arguments and output paths.

## Build and test entrypoints

| Entry | Purpose |
| --- | --- |
| `dev.py` | Cross-platform test dispatch, source SDK Python and development process lifecycle; use `--help` for commands |
| `test_tools.py` | Repository tooling tests; accepts unittest discovery flags (`-p`, `-k`, `-v`) |
| `test-sdk.sh` | SDK fast, mesh, regression, stress, Python2 and full gates |
| `test-editor.sh` | Editor fast, UI, regression, acceptance, tooling and full gates |
| `dev-python` | Source SDK experiments ([internal developer reference](../docs/references/dev-python.md)) |
| `python/python2_contract.py` | P3A inventory, source/artifact freeze, complete target map/signatures, agent configuration and P3A-0 exit gate |
| `python/python2_pairs.py` | Counterbalanced comparisons of frozen Python2 clients/Release binaries with the unchanged P0 workload and thresholds |
| `python/python2_p0_migration.py` | Freeze an explicit shared P3A API adapter for both generations; guard original inputs, operations, assertions, measurement core and policy |
| `../crates/zipzi-python2/tools/connected.py` | Build/install the independent pure client; real subprocess startup/session acceptance; `--install-only` supplies Editor's interpreter |
| `workspace/cargo_domain.py`, `workspace/package-domains.json` | Exhaustive SDK/Editor package selection and membership validation |
| `dev-editor.sh` | Editor development watcher and process lifecycle |
| `build-editor-app.sh` | macOS app bundle construction |
| `workspace/workspace.sh`, `workspace/workspace_tools.py` | Shared path and Cargo artifact handling |

See [test coverage](#test-coverage), [CI coverage](#ci-coverage) and
[Editor setup](../crates/zipzi-editor/README.md#develop-and-test). No mode on either domain test shell
script displays help. Select `full` explicitly for the expensive full gate.
Editor entrypoints resolve checkout paths themselves, including absolute calls
from another directory. The Python tooling runner also resolves its own checkout.

## Test coverage

Run commands from the repository root. `tools/workspace/package-domains.json` assigns every
owned workspace package to exactly one domain: 17 SDK packages (including Python2
and the Rust comparison runner) and 3 Editor packages. New workspace members must
be explicitly assigned; missing, stale, duplicate or overlapping names fail the
metadata coverage gate. Vendor packages are not workspace members.

```sh
python3 tools/workspace/cargo_domain.py --check
python3 tools/workspace/cargo_domain.py sdk check --locked --all-targets
python3 tools/workspace/cargo_domain.py editor check --locked --all-targets --features zipzi-editor/test-support
./tools/test-sdk.sh fast physics_editing::
./tools/test-editor.sh ui i18n::
```

`workspace/cargo_domain.py` runs from the repository root, preserves Cargo status and
arguments, and prevents package selectors from overriding its domain. It is a
Python standard-library tool; `uv run --locked python ...` uses the project's
Python environment. `fmt` selects only the domain's own packages, excluding vendor.

| Entry | Coverage |
| --- | --- |
| SDK `fast` | zipzi-sdk library and ordinary integration; optional filter |
| SDK `runtime` | CPU Runtime sessions, path reservations, transactions, jobs, panic, events, protocol/framing and retention contracts; optional filter |
| SDK `mesh` / `regression` | Daily alpha mesh / all 17 SDK libraries and integrations; skip only `alpha_mesh_stress::` |
| SDK `stress` | Entire fixed-seed alpha mesh matrix |
| SDK `python2` | New Runtime-backed package: refreshed native adapter, Local/Connected behavior, strict positive/negative types and generation drift |
| SDK `python2-connected` | New pure wheel in an isolated environment; real cold-start/ownership/recovery; optional `--editor-host /absolute/path/to/zipzi-editor` adds production desktop acceptance |
| SDK `full` | SDK formatting and complete 17-package Cargo test invocation (examples/doctests included) |
| Editor `fast` | zipzi-editor-core library and synthetic integration |
| Editor `ui` / `regression` | All three Editor libraries / libraries plus synthetic integration, with test-support |
| Editor `acceptance` | acceptance, native_render, headless_screenshot, animation_acceptance, timeline_acceptance、runtime_ipc_acceptance、runtime_handoff_acceptance（自动安装隔离 Python 客户端） |
| Editor `tools` | All repository tooling tests (automatic discovery) |
| Editor `full` | Editor formatting, tooling and complete three-package Cargo test invocation with test-support (examples/doctests included) |

SDK and Editor full extra arguments go to Cargo tests. Editor `fast` and `regression` omit
example compilation and doctests; `full` includes both. Focused Cargo arguments and arguments
after `--` retain their normal meaning. Editor Rust gates unset
`ZIPZI_DEV_SESSION_DIR` to isolate tests from live development sessions.
Formatting, Rust and tooling failures stop the relevant full gate and
propagate their exit status. Complete suites remain expensive: use focused modes
during iteration and reserve full runs for broad regression acceptance.

### CI coverage

Development CPU gates run on relevant pull requests on Windows and macOS. They run native Python entrypoint controls and focused SDK physics/Editor Core timeline tests. SDK CI, Editor CI and Prepare Python Packages remain manual-only. Ordinary branch pushes do not start the full CI suites. Only an explicitly pushed
`vX.Y.Z-alpha.N` tag starts Package macOS App and its GitHub prerelease upload.
See [the release guide](../docs/RELEASING.md) for versioning and tag commands.

For a downloadable app, open **Actions → Package macOS App → Run workflow**,
select the branch and start the run, or use
`gh workflow run macos-app.yml --ref <branch>`. This uses one `macos-15`
Apple Silicon runner to build a Release app, verify its contents, apply and
verify an ad hoc signature, and archive the bundle with executable permissions.
It creates a drag-to-Applications DMG installer and verifies the mounted App.
It also archives and verifies the corresponding workspace source and records
SHA-256 checksums. Manual runs upload artifacts only; Alpha tag pushes publish
the same verified artifacts to a GitHub prerelease.
It does not run the SDK/Editor test suites or build Python wheels. Download
`Zipzi-Editor-vX.Y.Z-alpha.N-macos-arm64` from the completed run's **Artifacts**
section and extract the enclosed app zip to obtain `Zipzi Editor.app`.
Artifacts are retained for three days; the job has a 90-minute timeout.
This is a development build without Developer ID signing or notarization.
Packaging success does not establish native runtime or UI acceptance.

Use **Actions → SDK CI / Editor CI → Run workflow** for the full checks, or
`gh workflow run rust.yml --ref <branch>` /
`gh workflow run editor.yml --ref <branch>`. Manual triggers become available
once the workflows are on the repository's default branch; no version tag is
required. Repeated runs of the same workflow for the same branch cancel the
previous pending or running workflow.

Prepare Python Packages builds the formal `zipzi` client and optional
`zipzi-native` backend, rebuilds wheels from their source archives and exercises
isolated Local/Connected installation. It uploads the packages and acceptance
report as three-day artifacts. No workflow uploads to PyPI or crates.io.

SDK CI retains Linux/macOS/Windows, all-target clippy, decoder safety without
native Core, complete SDK tests and the macOS WGPU blend reference. Maintained Python
package build and installed integration run in Prepare Python Packages. Cargo commands select only SDK packages.

Editor CI runs on macOS: package coverage, formatting, tooling, every-target
compilation with test-support, and core synthetic tests. A Swift Metal-device
probe controls UI/integration and native acceptance execution. A failed probe
is a job failure. An absent device produces an explicit warning and job summary;
it is not evidence that UI/native tests passed. On a Metal-capable runner,
regression and all configured acceptance targets must pass normally.

Licensed model data is not uploaded to CI. Optional missing-model cases retain
their existing skip behavior; `--nocapture` exposes their messages. Full model
acceptance stays in the local Editor acceptance/full gates after authorized data
is available. A passing hosted run does not establish local licensed-model or
unavailable GPU coverage.

### Repository tooling and Python SDK tests

Tooling implementations live in `tools/`; their regression tests live in
`tools/tests/test_*.py`. Both Editor `tools` and Editor `full` call the shared discovery
runner, including Python2 tooling controls. New test modules do
not require editing a shell-script module list.

```sh
uv run --locked python tools/test_tools.py
uv run --locked python tools/test_tools.py -p test_python2_tools.py -v
uv run --locked python tools/test_tools.py -k failure
```

SDK `fast`/`mesh` gates run CPU behavior, while observe/render tests need a GPU.
Editor UI and native gates require macOS and Metal. Full local licensed-model
requirements are documented in the [Editor README](../crates/zipzi-editor/README.md#local-model-acceptance).

Editor `runtime_ipc_acceptance` and `runtime_handoff_acceptance` exercise the independent `zipzi-python2`
client. The handoff case covers Python batch publication, native human refinement,
fresh-client continuation and shared history/Metal without licensed assets.
Acceptance/full prepares its installed pure wheel unless
`ZIPZI_CLIENT_PYTHON` supplies an isolated interpreter. It must contain the new
distribution without `zipzi_native`; the acceptance test checks this explicitly.

See [test layout and prerequisites](tests/README.md). These tests exercise
tooling, not the built SDK extension. Maintained Python SDK checks use
`test-sdk.sh python2` or `test-sdk.sh python2-connected`.

Daily SDK `mesh` and `regression` skip only `alpha_mesh_stress::`; the 65536-vertex
boundary remains in daily checks. `stress`, plain `cargo test -p zipzi-sdk` and
SDK `full` retain the complete fixed-seed matrix. Tests are not globally ignored.

## Validation and fixtures

| Tools | Purpose |
| --- | --- |
| `validation/validate_official_core.py`, `validation/probes/` | Official/Purism Core numerical comparison for MOC3 exports |
| `validation/compare_sdk_image.py`, `validation/compare_wgpu_blends.py` | Image differences and pinned WGPU blend-reference checks |
| `fixtures/create_*`, `fixtures/make_project_stress_fixture.py` | Reproducible shared fixture generation |
| `validation/check_cmo3.py`, `validation/cmo3/` | Cubism document inspection |
| `validation/run_*_probe.py`, `validation/run_*_integration.py` | Explicit format/runtime/visual acceptance experiments |
| `validation/run_purism_fuzz.sh`, `validation/check_purism_bundle.sh` | Purism fuzzing and single-header smoke checks |
| `packaging/package_source.py`, `packaging/verify_artifacts.py` | Source packaging, notice and built-artifact checks |

Reference captures in `fixtures/render_reference/` preserve model/texture hashes
and view settings; updates need new provenance and comparison evidence.
See [distribution](#local-distribution-artifacts) for packaging commands.

## Benchmarks and research tools

See [benchmark workloads and report interpretation](benchmarks/README.md) for
the native Mesh/Physics Properties and CPU property-editing-chain harnesses.

`benchmarks/timeline-benchmark.sh`, `benchmarks/timeline-number-benchmark.sh`, `benchmarks/properties-benchmark.sh`, `benchmarks/physics-properties-benchmark.sh`
and `benchmarks/window-benchmark.sh` run native Editor benchmarks. `benchmarks/property-chain-benchmark.py`
and `benchmarks/*-trace-report.py` measure and analyze editing/frame work. Native runs require
macOS and Metal; model-dependent runs need separately obtained local assets.

```sh
# Release input/draw, candidate latency, playback, rapid-seek, canvas-pan and dock-resize measurements:
./tools/benchmarks/timeline-benchmark.sh target/timeline-benchmark-run 600
# Timeline channel-number scrubbing, CPU/GPU stages and authoring comparison:
./tools/benchmarks/timeline-number-benchmark.sh target/timeline-number-benchmark-run 96
# Representative Mesh properties: 11 cases, including two color-picker cases:
./tools/benchmarks/properties-benchmark.sh target/properties-benchmark-run 30 120
# Physics text commits, continuous number drags, reflection and idle:
./tools/benchmarks/physics-properties-benchmark.sh target/physics-properties-run 40 120
# All 33 historical Mesh/color-picker cases:
ZIPZI_BENCH_PROPERTY_SUITE=mesh-full ./tools/benchmarks/properties-benchmark.sh target/properties-full-run 30 120
# Paired Release editing-chain totals, nested stage attribution and work counts:
uv run --locked python tools/benchmarks/property-chain-benchmark.py models/local/mao/runtime/mao_pro.model3.json target/property-chain-run --reader-policy unique
# Continuous Animation workspace drags in a native window (no per-input frame wait):
ZIPZI_BENCH_ANIMATION=1 ZIPZI_BENCH_ANIMATION_DRAG=canvas ./tools/benchmarks/window-benchmark.sh MODEL target/animation-canvas-drag 2400 240
ZIPZI_BENCH_ANIMATION=1 ZIPZI_BENCH_ANIMATION_DRAG=number ./tools/benchmarks/window-benchmark.sh MODEL target/animation-number-drag 2400 240
```

`runtime/runtime_inventory.py`, `runtime/runtime_baseline.py` and `runtime/runtime-migration-inventory.py`
support [Runtime preparation](../docs/runtime/README.md).
The independent [zipzi-python2 package](../crates/zipzi-python2/README.md) owns its
P0/P1 tools under `crates/zipzi-python2/tools/`. `test.py` builds its native adapter
and validates Local/Connected behavior and public static types; `baseline.py`
measures this package's selected workloads. `./tools/test-sdk.sh python2` is the
repository entrypoint.

Generated JSON inventories and reports belong under `target/`; plan directories
retain Markdown acceptance results only.

P3A starts with an explicit failure baseline, separate from the current API's
passing behavior suite. The following commands already exist; use a new output
filename for each run. `baseline` exits nonzero for failures, errors, skips or
changed inputs and still records the results. It does not rebuild fixtures or
claim installed-package, agent, performance or Editor acceptance.

```sh
uv run --locked python tools/python/python2_contract.py inventory --output target/python2/p3a-inventory.json
uv run --locked python tools/python/python2_contract.py freeze --output target/python2/p3a-freeze.json
./tools/test-sdk.sh python2 PythonSdk.test_shared_recipe
uv run --locked python tools/python/python2_contract.py baseline --output target/python2/p3a-baseline.json
uv run --locked python tools/test_tools.py -p test_python2_contract.py -v
```

`freeze --wheel /absolute/path/to/package.whl` records exact wheel identities;
repeat for both distributions. `--native`, `--host` and `--editor` select explicit
artifacts. Missing artifacts are recorded as missing, never as verified. The
inventory includes incidental exports for later removal review; `needs_review`
entries are not an approved final API catalogue.

`targets` writes the reviewed destination and full signature for every original
public path; `configure` freezes the G01–G08 experiment inputs without starting
agents. `check-freeze` validates both, the original failure evidence and copied
Release P0 binaries. See the [P3A-0 record](../docs/plans/python-sdk-rewrite/p3a-0.md)
for the completed acceptance result. Historical JSON artifacts have been removed
from the plan directories. These one-off historical checks require their original
inputs under `target/python2/p3a-reports/`; they are not prerequisites for later
implementation. Do not recreate removed reports merely to repeat an accepted stage.

`./tools/test-sdk.sh python2 --recovery` runs the focused P3A-1 module and strict
positive/negative types. The ordinary `python2` gate includes that module.
`python/python2_pairs.py --help` describes the layout for two copied client/Release
trees. Pairing uses the original workload and policy, alternates AB/BA, measures
latency before separate probes, records every run and rejects noise. Preserve
historical failures when diagnosing environment drift; paired evidence does not
overwrite them.

For an explicit public API migration, `python/python2_p0_migration.py --help`
prepares two copied clients running one identical worker and an immutable
`migration.json` under the requested temporary directory. Pass it to
`python2_pairs.py --migration`; the default path still requires the original
worker hash. Inputs, operations, assertions, samples, probe loops and original
thresholds remain guarded. `crates/zipzi-python2/tools/test.py --release --build-only
--output <directory>` builds/copies paired timing artifacts without repeating
behavior/type tests; add `--instrumented` with a different output for probes.

`runtime/check_editor_boundary.py` checks the R2 Editor ownership boundary;
`runtime/editor_benchmark_pairs.py` runs five alternating prebuilt Release pairs,
and `runtime/editor_benchmark_report.py` applies the frozen per-operation gates.
See [the benchmark guide](benchmarks/README.md#runtime-editor-pairs).

## Local distribution artifacts

The Editor source distribution is a **workspace archive**. Its Rust macros embed
icons and locales from `crates/zipzi-editor/assets/`; GPUI patches live outside
the Editor crate. The three Editor crates declare `publish = false`; use the
complete source layout below so the workspace patches and licenses are present.
This does not change Cargo package identities, APIs or licenses.

```sh
uv run --locked python tools/packaging/package_source.py target/releases/zipzi-2d-source.tar.gz
uv run --locked python tools/packaging/verify_artifacts.py --source target/releases/zipzi-2d-source.tar.gz
./tools/build-editor-app.sh editor debug
./tools/build-editor-app.sh editor release
uv run --locked python tools/packaging/verify_artifacts.py --app 'target/Zipzi Editor.app'
```

The source tool includes tracked and non-ignored new source files plus initialized
Purism sources, records hashes and checkout status, and refuses missing gitlinks,
escaping symlinks and tracked private/generated content. Initialize Purism before
packaging. Local models, the proprietary Cubism SDK under `third_party/`,
target, Python caches and machine Cargo config are excluded.
It retains all 20 crate paths, Editor crate assets, patches, docs and both license texts.
Extract the `zipzi-2d/` directory and build from that workspace root.

Python package artifacts are built and verified by the maintained
[zipzi-python2 workflow](../crates/zipzi-python2/README.md).

Both app profiles place the binary reported by Cargo in `Contents/MacOS`, the
application icon and MIT/GPL/licensing/notices in `Contents/Resources`, and retain
vendored license files under `vendor-notices/`. SVG icons and locales are embedded
in the binary rather than copied to a second runtime asset tree. The verifier
checks the actual embedded bytes and language text. The output path is reused
between profiles; retain a separate copy under `target/` when comparing both.

These commands create local unsigned artifacts. They do not upload releases,
notarize apps, change remote repositories or verify GitHub license presentation.


P3A-3/4/5 was committed without acceptance, by user request. Subsequent selected
checks and remaining gates are recorded in [P3A-6](../docs/plans/python-sdk-rewrite/p3a-6.md).
`./tools/test-sdk.sh python2 --discovery` selects the new focused collection,
Session lifecycle and offline catalog regressions. For installed-only checks,
run `tests/installed_contract.py --output /absolute/temporary/result.md` with the
pure-wheel environment's Python `-I`, or `tests/installed_examples.py local|connected`
with the corresponding normal wheel environment (paths relative to
`crates/zipzi-python2/`; Connected examples require `ZIPZI_PYTHON2_HOST`). The offline
audit exits nonzero for missing semantic docs and does not constitute a complete
manual D02 review. `--drop apply_authoring|commit|job` on the Connected example
harness runs public recovery examples with an external test-support host.
`uv run --locked python tools/python/python2_agent_tasks.py --help` describes fixed
installed-only packets, bounded recording, and a separate `--preflight <directory>`
service availability probe. Preparation runs no model; preflight executes no SDK task
and cannot mark G01–G08 passed. Fixture data lives in
`fixtures/python_sdk_agent/`; private controller manifests and raw run events belong
under `target/`, while plan directories keep Markdown conclusions only.

`tools/python/python2_agent_runner.py --help` runs installed-only diagnostic tasks
with a macOS Seatbelt MCP shell and a Connected IPC fault relay. It retains every
failed attempt. Per user instruction on 2026-10-26, token usage has no limit and is
reported after testing as input/output/total; unavailable counts are not zero-filled
or failed. Time, tool call/byte and log budgets still apply.
Local page/Plan interference and pre-close independent state/history capture are
connected through the controlled `sdk-python` workflow launcher. Shell responses
are paginated within 16 KiB; raw output stays in external logs and subsequent
pages consume tool calls. `python2_agent_matrix.py --help` freezes one 42-run
cohort and decides acceptance from functional results. See the
[agent fixture notes](../fixtures/python_sdk_agent/README.md) and
[P3A-6 results](../docs/plans/python-sdk-rewrite/p3a-6.md) for coverage and all
observed failures. The configured model/effort is `gpt-6.1-sol/high`; optional
service metadata has no effect on acceptance.

On 2026-10-26 the user removed the model/service identity gate. The runner,
preflight and matrix no longer require actual service revisions, identity
launchers or private metadata sidecars. Availability preflight requires a
completed CLI turn. Missing or changing service metadata is report-only; task
success, independent verification and functional hard stops determine results.
See the [metadata notes](../fixtures/python_sdk_agent/README.md#modelservice-metadata).
