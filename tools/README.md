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
| `python/` | Python client contract and agent test helpers |
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
| `test-sdk.sh` | SDK fast, mesh, regression, stress, Python and full gates |
| `test-editor.sh` | Editor fast, UI, regression, acceptance, tooling and full gates |
| `dev-python` | Source SDK experiments ([internal developer reference](../docs/references/dev-python.md)) |
| `../crates/zipzi-python/tools/connected.py` | Build/install the independent pure client; real subprocess startup/session acceptance; `--install-only` supplies Editor's interpreter |
| `workspace/cargo_domain.py`, `workspace/package-domains.json` | Exhaustive SDK/Editor package selection and membership validation |
| `dev-editor.sh` | Editor development watcher and process lifecycle |
| `build-editor-app.sh` | macOS app bundle construction |
| `runtime/native_project_macos.py` | Opt-in real Editor process and LaunchServices file-event acceptance; `--dialogs` verifies English native panels. `--finder-default --case cold` temporarily uses release document metadata and opens the selected files through Finder with Accessibility access; an independent guardian restores the previous handler and removes the temporary bundle. Resident Finder acceptance remains unfinished. |
| `packaging/package_windows.py` | Build and verify a Windows x86-64 portable archive from a compiled Editor |
| `packaging/build_project_preview.py` | Build macOS Quick Look extensions / Windows Shell preview DLL; `--test` checks native providers |
| `packaging/collect_python.py` | Verify acceptance reports and collect the Python release distributions |
| `packaging/reuse_release.py` | Select a successful default-branch build and verify its commit, version and publication hashes before tag promotion |
| `packaging/release_downloads.py` | Prepare the Mac DMG, Windows ZIP/installer, signed update assets, full source and matching checksums; generate desktop download links and Python install instructions |
| `packaging/ci_updates.py`, `packaging/publish_updates.py` | Validate opt-in Alpha signing configuration and advance the fixed authenticated GitHub update feeds after versioned releases |
| `workspace/workspace.sh`, `workspace/workspace_tools.py` | Shared path and Cargo artifact handling |

See [test coverage](#test-coverage), [CI coverage](#ci-coverage) and
[Editor setup](../crates/zipzi-editor/README.md#develop-and-test). No mode on either domain test shell
script displays help. Select `full` explicitly for the expensive full gate.
Editor entrypoints resolve checkout paths themselves, including absolute calls
from another directory. The Python tooling runner also resolves its own checkout.

### Editor development watcher

`uv run --locked python tools/dev.py editor watch` builds and starts the Editor,
then uses `watchexec` native file notifications on macOS and Windows. Install
the watcher with `cargo install --locked watchexec-cli`, or
`brew install watchexec` on macOS. `editor run` and `editor reload` do not
require it.

The watcher covers source and assets under `crates/`, `vendor/`, and `tools/`,
plus root build configuration. Changes received during a build queue another
reload. Failed builds keep the current Editor running, and closing the Editor
stops the watcher. The Python launcher retains process ownership and reload
locking; it no longer scans the source tree periodically.

## Test coverage

Run commands from the repository root. `tools/workspace/package-domains.json` assigns every
owned workspace package to exactly one domain. New workspace members must
be explicitly assigned; missing, stale, duplicate or overlapping names fail the
metadata coverage gate. Vendor packages are not workspace members.

```sh
python3 tools/workspace/cargo_domain.py --check
python3 tools/workspace/cargo_domain.py sdk check --locked --all-targets
python3 tools/workspace/cargo_domain.py editor check --locked --all-targets --features zipzi-editor/test-support
./tools/test-sdk.sh fast native_image
./tools/test-editor.sh ui i18n::
```

`workspace/cargo_domain.py` runs from the repository root, preserves Cargo status and
arguments, and prevents package selectors from overriding its domain. It is a
Python standard-library tool; `uv run --locked python ...` uses the project's
Python environment. `fmt` selects only the domain's own packages, excluding vendor.

| Entry | Coverage |
| --- | --- |
| SDK `fast` | New zipzi-sdk library and `authoring` native-workflow integration; optional filter |
| SDK `runtime` | CPU Runtime library, resident host, transport, source identity and sampling tests |
| SDK `mesh` / `regression` | Daily alpha mesh / all SDK libraries and integrations; skip only `alpha_mesh_stress::` |
| SDK `stress` | Entire fixed-seed alpha mesh matrix |
| SDK `python` | New Runtime-backed package: refreshed native adapter, Local/Connected behavior, strict positive/negative types and generation drift |
| SDK `python-connected` | New pure wheel in an isolated environment; private IPC, Session ownership and recovery; optional `--editor-host /absolute/path/to/zipzi-editor` adds production desktop acceptance |
| SDK `full` | SDK formatting and complete SDK-domain Cargo test invocation (examples/doctests included) |
| Editor `fast` | zipzi-editor-core library and synthetic integration |
| Editor `ui` / `regression` | All three Editor libraries / libraries plus synthetic integration, with test-support |
| Editor `acceptance` | acceptance, native_render, headless_screenshot, animation_acceptance, timeline_acceptance、runtime_ipc_acceptance、runtime_handoff_acceptance（自动安装隔离 Python 客户端） |
| Editor `tools` | All repository tooling tests (automatic discovery) |
| Editor `full` | Editor formatting, tooling and complete three-package Cargo test invocation with test-support (examples/doctests included) |

SDK fast and runtime cover focused v4 targets. The broad SDK and Editor gates
still include legacy tests and examples awaiting migration. The
[authoring plan](../docs/archive/plans/authoring-architecture.md) records the verification scope.

SDK and Editor full extra arguments go to Cargo tests. Editor `fast` and `regression` omit
example compilation and doctests; `full` includes both. Focused Cargo arguments and arguments
after `--` retain their normal meaning. Editor Rust gates unset
`ZIPZI_DEV_SESSION_DIR` to isolate tests from live development sessions.
Formatting, Rust and tooling failures stop the relevant full gate and
propagate their exit status. Complete suites remain expensive: use focused modes
during iteration and reserve full runs for broad regression acceptance.

### CI coverage

Development gates run on relevant pull requests and pushes to `main`. Linux jobs
run tooling, focused SDK/Editor-core tests and Python Local/Connected checks in
parallel. macOS and Windows separately compile the native Editor; Windows also
checks presentation scheduling. These jobs and the manual SDK/Editor workflows
cache Rust dependencies in `target/ci-cargo`. SDK CI and Editor CI remain manual.

**Alpha release** prebuilds on relevant `main` pushes and manual dispatches without
publishing. Mac app, Windows app and Python on each platform build in four
parallel jobs using the standard GitHub-hosted runners and all available CPU
cores. App and Python dependency caches use `target/alpha-app-cargo` and
`target/alpha-python-cargo`, independently per OS. Successful default-branch
runs and manual builds save caches for subsequent builds and tag runs to restore.
Manual app builds retain dependency caches after a failed test as well.

Only a pushed `vX.Y.Z-alpha.N` tag publishes to GitHub and PyPI. When a successful
default-branch build for that exact commit still has all its artifacts, the tag
run skips native compilation and verifies its build manifest, update configuration, version and every
publication file hash before publishing those same bytes. Missing, expired,
failed, PR or other-commit builds cannot be reused; the workflow builds normally.
Non-tag build artifacts last one day; tag artifacts last seven days. See
[the release guide](../docs/RELEASING.md) for setup and recovery.

The public GitHub release uploads the Mac DMG, Windows portable ZIP, complete
source and their checksums by default. Opt-in signed desktop updates replace the
ZIP with an installer and add authenticated feeds and packages. Only successful
Alpha tags advance the fixed `desktop-alpha-updates` release; main/manual builds
only prepare artifacts. See the [update configuration](../docs/references/desktop-updates.md#github-alpha-ci-configuration).
Python distributions remain on PyPI and in the
validated Actions bundle. A generated download section precedes GitHub's
automatic changelog, so users do not need to choose between wheel files or
duplicate Mac formats. It requires no per-version notes maintenance.

Python still builds from extracted source archives, refreshing materialized
timestamps so cached workspace crates cannot hide changed source. Its schema
exporter and acceptance host share one release-profile Cargo invocation and
feature resolution. Generation checks use the freshly compiled export. Wheel
builds use a stable base Python interpreter to avoid temporary-venv PyO3
rebuilds. macOS wheel linker flags still require separate compatible artifacts.
`commands.json` records command durations for comparison; cache hits always
depend on Cargo's source, feature and compiler fingerprints.

Use **Actions → SDK CI / Editor CI → Run workflow** for the full checks, or
`gh workflow run rust.yml --ref <branch>` /
`gh workflow run editor.yml --ref <branch>`. These workflows are manual-only.
Repeated runs of the same workflow for the same branch cancel the previous
pending or running workflow.

SDK CI retains Linux/macOS/Windows, all-target clippy, decoder safety without
native Core, complete SDK tests and the macOS WGPU blend reference. Maintained Python
package build and installed integration run in Alpha release. Cargo commands select only SDK packages.

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
runner, including v4 code generation and ownership checks.

```sh
uv run --locked python tools/test_tools.py
uv run --locked python tools/test_tools.py -p test_python_tools.py -v
uv run --locked python tools/test_tools.py -k failure
```

SDK `fast`/`mesh` gates run CPU behavior, while observe/render tests need a GPU.
Editor UI and native gates require macOS and Metal. Full local licensed-model
requirements are documented in the [Editor README](../crates/zipzi-editor/README.md#local-model-acceptance).

Editor `runtime_ipc_acceptance` and `runtime_handoff_acceptance` exercise the independent `zipzi-python`
client. The handoff case covers Python batch publication, native human refinement,
fresh-client continuation and shared history/Metal without licensed assets.
Acceptance/full prepares its installed pure wheel unless
`ZIPZI_CLIENT_PYTHON` supplies an isolated interpreter. It must contain the new
distribution without `zipzi_native`; the acceptance test checks this explicitly.

See [test layout and prerequisites](tests/README.md). These tests exercise
tooling, not the built SDK extension. Maintained Python SDK checks use
`test-sdk.sh python` or `test-sdk.sh python-connected`.

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

The [Python package](../crates/zipzi-python/README.md) owns v4 generation and
testing. The generator compiles the independent `zipzi-contract` exporter.
`--check` verifies the manifest and output bytes, and `--schema` replays an
exported contract. `test_python_codegen.py` checks output drift, replay, file
ownership and static types.

`./tools/test-sdk.sh python --recovery` runs the v4 same-request recovery cases.
The ordinary `python` gate also checks generated types.

`runtime/editor_no_window.py BINARY --output target/no-window-check` checks a
macOS Editor process stays alive without creating WindowServer windows. It
launches and terminates its own child; an already-built Editor is required.

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
It retains all 21 crate paths, Editor crate assets, patches, docs and both license texts.
Extract the `zipzi-2d/` directory and build from that workspace root.

Python package artifacts are built and verified by the maintained
[zipzi-python workflow](../crates/zipzi-python/README.md).

Both app profiles place the binary reported by Cargo in `Contents/MacOS`, the
application icon and MIT/GPL/licensing/notices in `Contents/Resources`, and retain
vendored license files under `vendor-notices/`. SVG icons and locales are embedded
in the binary rather than copied to a second runtime asset tree. The verifier
checks the actual embedded bytes and language text. The output path is reused
between profiles; retain a separate copy under `target/` when comparing both.

The macOS builder includes sandboxed, signed Thumbnail and Preview extensions in
`Contents/PlugIns`. Sign the enclosing app after its extensions; preserve their
sandbox entitlements by signing the outer app without recursive re-signing.
Windows portable packaging requires `--preview-dll`; its registration script
registers both Open With and static Shell previews. See the [native preview
instructions](../native/project-preview/README.md) for builds and platform gates.

The enclosing macOS app still needs its final signature. These commands do not upload releases,
notarize apps, change remote repositories or verify GitHub license presentation.

`packaging/package_updates.py` creates Velopack installers and authenticated
update feeds for Windows and macOS. See [desktop updates](../docs/references/desktop-updates.md)
for the pinned packaging tool, build-time public configuration, private signing
inputs and installed upgrade gates. It does not publish artifacts.


The v4 `./tools/test-sdk.sh python --discovery` gate exercises pagination and
Session attachment, detachment and close. Use `./tools/test-sdk.sh python-connected`
for the installed client and `./tools/test-sdk.sh python --observation` for GPU
observation.

## Installed SDK agent evaluation (v4)

`python/python_agent_v4.py` runs fresh Codex CLI agents against normal installed
wheels. Historical P3A results are retained in [the archive](../docs/archive/plans/python-sdk-rewrite/p3a-6.md).
It currently requires macOS Seatbelt (`sandbox-exec`) and an authenticated Codex
CLI. The CLI's default model/effort is used without override; version and actual
usage are recorded. Tokens have no limits. Each task has finite time/tool limits.

Build a client wheel and a normal optional native wheel, then select a new output
directory for each cohort (do not overwrite an earlier wheel or result):

```sh
uv build --wheel --out-dir target/python/agent-wheels crates/zipzi-python/client
uv run --locked maturin build --manifest-path crates/zipzi-python/Cargo.toml --interpreter .venv/bin/python --out target/python/agent-wheels
uv run --locked python tools/python/python_agent_v4.py suite --output target/python/agent-cohort-1 --wheel target/python/agent-wheels/zipzi-0.1.0a8-py3-none-any.whl --wheel target/python/agent-wheels/zipzi_native-0.1.0a8-cp314-cp314-macosx_11_0_arm64.whl --host target/python/local_host_fixture
```

Replace filenames with the wheels actually built for the current platform/version.
The host fixture is built by `./tools/test-sdk.sh python`. `suite --environment
connected` or `--environment local` selects one environment; `run --help` describes
single-case execution. `--parallel` accepts 1–3. Keep wheel and host inputs immutable
while a cohort runs. Failures are retained; a rerun uses a new output directory.

The participant receives outcome goals and input projects, an isolated virtual
environment, installed package documentation, and one MCP shell. Local installs
both wheels; Connected installs only the pure client. Its shell cannot read the
repository/private controller, change installed packages, or access the Internet.
Actual isolation probes and package/harness/host fingerprints are retained.
Workflow scripts run through `sdk-python work/script.py`, which records actual
public-call results/errors over a private inherited channel. These observations
do not change SDK outcomes. This is cooperative evaluation, not a hostile-code
security boundary against Python introspection.

`python/python_agent_v4_fixture.py` prepares and independently grades four tasks:
300-record pagination with duplicate names and exact-target editing; multi-chunk
RGBA/mesh edits with Undo/Redo; real motion authoring and preview-clock sampling;
and stale-source/failing-Draft/zero-timeout Job handling. A separate installed
judge process reopens outputs, compares unrelated content and connected decoys,
and checks transient claims against trusted call observations. Agent completion
text never decides correctness. Animation expectations use independent curve math
and the documented canonical playback clock, not measured runtime outputs.
Sample readback evidence is assembled by Blob ID and byte offset, so partial or
out-of-order reads count once they cover the complete payload. Async recovery
checks associate the requested create command with its Job, zero-timeout wait,
completed Draft source and subsequent publication. Deadline cleanup stops the
independent workflow process group before artifact verification. Delivered tool
text uses the same UTF-8 serialization as the 16 KiB pagination budget.

Per-case controller directories retain prompts, CLI events, shell attempts,
observations, wheel/document hashes, isolation probes, verification and results.
`summary.json` keeps completed results, including infrastructure failures before
agent dispatch; those must be distinguished from SDK task failures in analysis.
A small diagnostic sample does not establish a population success rate.
