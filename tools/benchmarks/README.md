# Benchmark guides

Run commands from the repository root. These guides document existing harnesses
and how to interpret their output; measured results belong in
[`docs/archive/benchmarks/`](../../docs/archive/benchmarks/).

| Guide | Use it for |
| --- | --- |
| [Manual Mesh drag](#manual-mesh-drag) | Continuous model-space topology dragging, CPU cost and native presentation |
| [Native Properties: Mesh and Physics](#properties-native-window-benchmark) | Real-window input, UI/Metal work and presentation correlation |
| [Property editing chain](#property-editing-chain-benchmark) | Synchronous CPU editing cost, stage attribution and ownership controls |
| [Whole-model parameter overlays](#whole-model-parameter-overlays) | Parameter dragging with every visible Mesh selected, rendering ablations and CPU/GPU attribution |

Choose checks using the [UI performance reference](../../docs/references/ui-performance.md).
See the [tool index](../README.md#benchmarks-and-research-tools) for Timeline,
window and offscreen benchmark commands. Native window harnesses require macOS,
Metal and the documented local model; CPU chain measurements do not measure
window or GPU latency.

## Manual Mesh drag

The CPU probe uses the normal Editor gesture, candidate, panel-index and preview
paths. It validates changed inputs and cancellation without saving the model:

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo run --locked --release -p zipzi-editor-core \
  --example manual_mesh_drag_benchmark -- models/local/mao/runtime/mao_pro.model3.json ArtMesh215 60
cargo run --locked --release -p zipzi-sdk --example manual_mesh_benchmark
```

The Editor probe accepts `MODEL [MESH_ID_OR_RUNTIME_ID] [SAMPLES]`. It focuses an
editable mesh and moves one interior vertex towards an incident triangle's center,
using the same frozen pose and UV transfer as ordinary input. The SDK probe uses
1,089 vertices and 58 stored channels and counts cumulative allocator requests;
those bytes are not peak RSS. CPU probes do not measure presentation or FPS.

For the native window, enable the manual workload (exclusive with other workloads):

```sh
env -u ZIPZI_DEV_SESSION_DIR ZIPZI_BENCH_MANUAL_MESH=1 ZIPZI_BENCH_WARMUP=20 \
  ./tools/benchmarks/window-benchmark.sh \
  models/local/mao/runtime/mao_pro.model3.json target/manual-mesh-window 120 120
```

It uses the production pointer path, verifies every input produces a model
candidate/evaluation, and commits then undoes the gesture without saving. Fixture
selection records the mesh ID, vertex ID and vertex count. A vertex moves along an
interior triangle direction instead of an arbitrary circle crossing the outline.
The trace identifies this as `manual_mesh: true`, workload 5. Preserve raw traces,
binary hashes and reports. Keep the same chosen mesh, vertex, viewport and input
trajectory across before/after runs; discard failed or incomplete runs. Build
outside sampling intervals. Frame reports measure from GPUI dispatch through
correlated Metal presentation, excluding OS input delivery and input-to-photon.

## Windows GPUI CPU and image-upload diagnosis

Use the production Editor's automatic window workload after building; do not
compile during measurement. It dispatches GPUI events inside a native window,
not OS mouse input. The default workloads are pan (1), zoom (2), and opacity (3).
For a control that only repaints the existing canvas, set
`ZIPZI_BENCH_UI_ONLY=1` (workload 0): no new preview requests or image uploads
should appear after warmup. This isolates the existing Editor/Kit UI tree from
model rendering/readback; it is not a benchmark of an empty GPUI application.

Example from PowerShell, with an otherwise unused output directory/profile:

```powershell
cargo build --locked --release -p zipzi-editor
New-Item -ItemType Directory -Force target/gpui-probe | Out-Null
Remove-Item Env:ZIPZI_DEV_SESSION_DIR -ErrorAction SilentlyContinue
$env:ZIPZI_RUNTIME_PROFILE='gpui-probe'
$env:ZIPZI_CONFIG_DIR=Join-Path $PWD 'target/gpui-probe/config'
$env:ZIPZI_FRAME_TRACE=Join-Path $PWD 'target/gpui-probe/trace.jsonl'
$env:ZIPZI_WINDOW_BENCH='1'
$env:ZIPZI_BENCH_MODEL=(Resolve-Path models/local/mao/runtime/mao_pro.model3.json).Path
$env:ZIPZI_BENCH_SAMPLES='180'
$env:ZIPZI_BENCH_WARMUP='30'
$env:ZIPZI_BENCH_HZ='120'
uv run --locked python -c "import subprocess; subprocess.run(['target/release/zipzi-editor.exe'], check=True)"
uv run --locked python tools/benchmarks/windows-gpui-trace-report.py target/gpui-probe/trace.jsonl --output target/gpui-probe/summary.json
```

Keep the test window active. Repeat with separate trace/config paths for Dev,
Release and the UI-only control. Preserve raw traces and binary hashes; alternate
profile order when repeating. No model is saved by this harness. The report
requires a completed run, all declared workload samples and zero lost events.
It preserves inputs without a joined new image and repeated platform calls for
the same CPU scene. Test the reporter with
`uv run --locked python tools/test_tools.py -p test_windows_gpui_trace_report.py`.

`ui_draw_ms` includes Editor/Kit element construction, prepaint/layout and paint.
`image_atlas` spans atlas lookup and, on misses, allocation/upload API work; it
does not time asynchronous GPU transfer completion. `platform_draw_cpu_ms` covers
the platform drawing call, including D3D11 submission and synchronous Present
work on Windows. None of these is a GPU timestamp or DWM/scanout measurement.
`input_to_first_image_platform_return_ms` ends when the CPU platform call returns;
it excludes OS input delivery and actual screen presentation. A requested 120 Hz
input timer is not evidence of 120 Hz input delivery or display/FPS. Build,
prepaint, paint and atlas scopes are nested; do not add their independent P95s.

## Windows visible Timeline diagnosis

For visible Timeline diagnosis on Windows, build with `test-support` and use
the same automatic window harness with `ZIPZI_BENCH_ANIMATION=1`.
`ZIPZI_BENCH_TIMELINE_VARIANT=Natural|Dense` reuses the existing acceptance
fixture generator in a disposable copy of the supplied model. The original
assets are never saved. With `ZIPZI_BENCH_ANIMATION_DRAG` unset it measures
playback; `canvas` and `number` dispatch the existing production drag handlers.
The `number` case slides the Timeline `ParamAngleX` channel value through the
real candidate-key editing path. Number/canvas drags honor `ZIPZI_BENCH_WARMUP`
and emit workload-9 input ticks. On Windows, use
`windows-gpui-trace-report.py` to join those ticks through
`animation_enqueued.input` to shared-texture paints and CPU platform returns.
The [2026-10-30 real Timeline baseline](../../docs/archive/benchmarks/2026-10-30-windows-timeline-number.md)
is the reference for subsequent Modeling interaction comparisons: UI draw P50
5.26 ms and input-to-associated-platform-return P50 23.83 ms (medians across
three per-run P50s). Preserve the documented input cadence, camera/workspace,
association counts and measurement boundary when comparing against it.
The additional `timeline_scroll`, `timeline_zoom`, `timeline_seek` and
`timeline_key` probes verify navigation/candidate changes and preserve authoring
history. These visible probes do not cover the macOS headless Timeline pixel
restoration and persistence suite.

Add `bench-instrumentation` and `ZIPZI_BENCH_ANIMATION_DETAIL=1` to capture
synchronous core/SDK spans for each canvas/number input. These spans report
inclusive/self wall time on the UI thread; they do not measure GPU work or
presentation FPS. Disable detailed recording for primary FPS comparisons. Keep
builds out of the sampling interval and retain the unchanged driver and binary
for before/after measurements.

## Whole-model parameter overlays

Build once, then benchmark the saved Release executable without concurrent builds.
The default workload uses Mao, 30 warmup inputs and 360 measured inputs at 120 Hz,
with three rounds in alternating order. Keep the benchmark window in front.

```sh
cargo build --locked --release -p zipzi-editor --features test-support,bench-instrumentation
python3 tools/benchmarks/parameter-overlay-benchmark.py \
  --binary target/release/zipzi-editor --output target/parameter-overlays
python3 tools/benchmarks/parameter-overlay-benchmark.py \
  --binary target/release/zipzi-editor --output target/parameter-overlay-detail \
  --cases detail_unselected detail detail_no_paint --rounds 1
```

The main comparison disables detailed timers. Cases include no selection, all
visible unlocked Meshes selected, the same selection with painting skipped, only
vertices, and only lines. Skipping painting retains overlay construction and
selection-dependent panel behavior. The all-Mesh fixture matches enclosing the
whole model with object marquee selection; it does not select individual vertices.
Every run verifies the parameter deforms model positions and saves the parameter
identity, selection, vertex/edge counts, raw trace and background-process samples.
`--model` selects another local model; the first visible slider is used and audited.

The default GPUI fallback builds independent straight edges directly and groups
consecutive marker quads into paint layers. To compare the previous path in the
same binary, pass `--unlayered-markers --tessellated-edges` to the runner (each can
also be measured separately). Their automatic-driver equivalents are
`ZIPZI_BENCH_UNLAYERED_MARKERS=1` and `ZIPZI_BENCH_TESSELLATED_EDGES=1`; the trace
records both settings. Detailed runs split `overlay.gpui_mesh_edges`,
`overlay.gpui_lines` (semantic/control lines), and `overlay.gpui_markers`.
`ZIPZI_BENCH_LEGACY_EDGE_BOUNDS=1` keeps the direct triangles but restores their
old per-triangle bounding-box accumulation, isolating the batched-bounds change
in the same binary. It requires the automatic parameter workload and is recorded
as `parameter_overlay.legacy_edge_bounds`; leave tessellated edges disabled.
`ZIPZI_BENCH_EAGER_VIEWPORT_LAYOUT=1` restores unconditional canvas prepaint
layout writes. It requires the automatic parameter workload and is recorded as
`parameter_overlay.eager_viewport_layout`. Change one switch at a time and keep
detailed recording identical in each pair.
`ZIPZI_BENCH_VIEWPORT_STYLE=animation` gives the Modeling viewport Animation's
image/navigation presentation: no Modeling overlay construction, editing toolbar,
tool handler or object menu. The shared navigation content factory is also used
by the real Animation viewport. This diagnostic keeps Modeling frame bindings,
the model worker, selection and parameter-slider input intact; it does not switch
to the animation motion worker. The default is `modeling`. It requires the
automatic parameter workload and is recorded as `parameter_overlay.viewport_style`.
Compare against both normal Modeling and `ZIPZI_BENCH_OVERLAY_PAINT=none`, which
still constructs the Modeling overlay. Keep viewport dimensions and selection
identical, and verify newly presented frames and actual mesh deformation.
See the [Windows viewport comparison](../../docs/references/windows-viewport-style.md)
for the 2026-10-30 controlled results and reproduction commands.
`ZIPZI_BENCH_PARAMETER_INPUT=track|number|pad` selects the real Modeling
track, SlideNumberInput or linked two-axis pad (default `track`). `pad` links the
first two adjacent parameters in the isolated imported document before warmup;
its gesture changes both axes. Audits require a changed displayed mesh and,
for `number`/`pad`, an active drag and unchanged history; pad also requires two
changed displayed parameters. This switch requires the automatic parameter-only
workload. Keep the full Modeling presentation for comparisons against Timeline.
The Windows report includes outer input dispatch/effect-flush time, interval P99
and counts above 25 ms. Use these alongside interval P95 and input-to-associated
CPU platform return; the latter is not screen latency. See the
[shared parameter preview investigation](../../docs/archive/benchmarks/2026-10-30-windows-parameter-chain.md).
The `vertices` case skips fallback mesh edges as well as control lines on Windows.
Earlier Windows `vertices` traces that still included mesh edges are not a pure
vertex ablation. Preserve those raw results rather than relabeling them.

The Python launcher above uses macOS `caffeinate`/`ps`. On Windows, launch the
same production automatic driver with the isolated subprocess setup from the
Windows section and set `ZIPZI_BENCH_PARAMETERS=1`, selection/paint variables
and the reference switches as needed. `diagnose` in this report module accepts
completed Windows traces for CPU attribution; actual FPS still needs independent
PresentMon evidence. CPU time and process working set do not measure GPU time
or GPU memory.

Detailed runs capture overlay construction by caller, nested CPU stage inclusive
and self times, plus actual Metal command GPU timestamps on a separate diagnostics
worker. They report UI-thread wall time, not thread CPU time. Compare detailed
runs separately because timing and work counters add overhead. The driver retains
warmup/setup in raw traces but excludes them from measured stage distributions;
it preserves all request outcomes through the shared frame report. Background
compiler activity is recorded as `interfered`, and such runs require review or
fresh sampling before making performance claims.

Detailed traces also include independent `ui.*` scopes for the editor root,
native menu refresh, viewport, parameters, parameter rows, properties, and
document panels. Their duration is `fields.end_ns - at_ns`; nested scope
percentiles must not be added. Compare counts as well as durations to distinguish
repeated canvas construction from selection-dependent menu work.

Preview presentation joins use the request identity stored on the live Metal
resource, emitted as `compose_surface.fields.preview`. Zero denotes a non-preview
surface. Raw texture addresses are insufficient: an overlay may reuse a retired
preview's address. The reporter rejects ambiguous legacy captures; keep their
independent CPU timings, but repeat preview FPS/latency measurements with identity
tags. Automatic parameter runs also support `ZIPZI_BENCH_EAGER_CANVAS=1` and
`ZIPZI_BENCH_UNCACHED_CREATION_MENUS=1` for controlled refresh/menu ablations.

## Properties native-window benchmark

Use the Mao source model, a Release build and an isolated configuration. The default
Mesh suite runs 11 representative cases with 30 measured inputs per case at a
target 120 Hz. Physics runs 12 cases with 40 measured inputs per case:

```sh
./tools/benchmarks/properties-benchmark.sh target/properties-run 30 120
./tools/benchmarks/physics-properties-benchmark.sh target/physics-properties-run 40 120
```

`ZIPZI_BENCH_MODEL` overrides the model. `ZIPZI_BENCH_PROPERTY_CASE` selects one
case from the checklist below, including an individual `physics_*` case.
`ZIPZI_BENCH_PROPERTY_SUITE` selects `mesh` (default), `physics`, or `mesh-full`.
The default Mesh cases are name, runtime_id, part, masks, opacity_text,
opacity_drag, color_blend, enabled, picker_open_close, picker_hex, and idle.
`mesh-full` retains all 33 historical Mesh/color cases for explicit detailed checks.
`ZIPZI_BENCH_WARMUP` overrides the scripts' defaults: 3 for Mesh and 5 for Physics.
The runner requires an unlocked Mac, a visible,
active native window, and real Metal. Avoid input and concurrent builds while it
runs. It chooses the visible mesh with the most vertices, restores committed
edits outside each measured phase, never saves the source, and quits at the end.

### Physics coverage

Physics selects the first imported group with input/output channels and a non-root
node through the real native group list. Cases cover group name, input weight
(text/continuous drag), output weight (text/drag), output scale, node radius,
node mobility (text/drag), normalization center, input reflection, and static idle.
Numbers use native keyboard/IME dispatch and Slide Number Input pointer events.
The driver verifies requested text values, actual candidate changes, no commits
during a drag, exactly one commit on release, and Undo restoration outside timing.
It never saves the model. Physics uses 5 warmup plus 40 measured inputs by
default; their sum must be at most 50 to preserve every Undo step. Imported Physics
UUIDs may differ per load; compare the same source file and group/channel indices.
Both version builds must use the identical benchmark files; changes to the driver alone are not product performance changes.

The trace declares `property_cases` explicitly. Historical traces without this
field retain the original single-case/33-case interpretation. Physics idle does
not invent model texture or presentation timings when no new preview is needed.

### Mesh checklist (full suite)

| Cases | Input path |
| --- | --- |
| `name`, `runtime_id` | GPUI text entry, replacement and Enter commit |
| `part` | Open searchable select, enter query, debounce, select; verify requested ID |
| `masks` | Search and add a different mesh, remove the first reference; verify added ID |
| `opacity_text`, `opacity_drag` | Text commit and uninterrupted Slide Number Input gesture |
| `multiply_hex`, `multiply_reset` | Inline HEX commit and restore-default suffix |
| `color_blend` | Cycle all 18 supported modes; verify each requested mode |
| `enabled` | Repeated checkbox changes |
| `picker_open_close` | Open the production popover and dismiss with Escape |
| `picker_tabs` | Switch RGB/HSV tabs |
| `picker_hex`, `picker_invalid_hex` | Live valid HEX preview and alternating invalid/valid drafts |
| `picker_hue`, `picker_sv` | Continuous color-wheel hue and SV-triangle drags |
| `picker_rgb_{0,1,2}_text`, `picker_hsv_{0,1,2}_text` | Edit every RGB and HSV numeric field |
| `picker_rgb_{0,1,2}_drag`, `picker_hsv_{0,1,2}_drag` | Scrub every RGB and HSV numeric field |
| `picker_done`, `picker_cancel` | Change color, confirm/cancel, reopen |
| `picker_swatches` | Hover original/current swatches, including tooltip dwell |
| `idle`, `picker_idle` | Blurred, static panel and open picker baselines |

Multiply Color is RGB; its picker has no Alpha control. Color-channel gestures
start from a non-grey color. Number scrubs use the existing Timeline number
probe's repeated 7-pixel movement pattern so quantized controls receive meaningful
changes. Continuous inputs never wait for a preview frame. Searchable selects
wait for their actual debounce; popup lifecycle cases wait for their new UI.

Only observation and pointer-isolation hooks require `test-support`. The native
runner reads observed bounds, dispatches actual pointer/key/IME text events, and
does not call headless helpers that force frames after each input. Pointer
isolation follows the existing animation-number benchmark: native AppKit hover
cannot interrupt scripted drags. Normal application builds do not include these
observation hooks.

### Measurements and interpretation

The output contains `environment.txt`, build/application logs, `trace.jsonl`,
`requests.json`, and `summary.json`. Each case reports measured input count,
observed value changes, timer lateness, actual input intervals, UI draw counts,
window presentations, request outcomes, and p50/p95/p99/max durations.

The reporter reuses `frame-trace-report.py` for request, texture lifetime and
presentation correlation. Metrics include scene evaluation, resource preparation,
encoding, model-preview GPU, UI CPU draw, and window-composition GPU. It sums
nested native event-handler scopes per input separately from interaction wall
time, which includes intentional debounce and popup waits. GPU durations are
Metal timestamps, not CPU submission duration.

Tabs, invalid drafts, tooltips and idle can legitimately reuse a model texture.
Their UI and composition work is reported by phase; the reporter does not invent
input-to-presentation latency for those changes. Missing latency counts stay
visible. The phase includes 250 ms of trailing settle time; idle rates should
use the reported phase duration.

As in the existing UI benchmark, missing samples/end records, dropped trace
events, failed native presentations, GPU errors, edit errors and incomplete runs
are failures. Debug measurements cannot pass Release validation. Cases that edit
values must demonstrate a changed property or live color/number value. Expected
no-change operations retain their zero change count.

Compare CPU and GPU stages with the existing 8.33 ms reference budget and retain
over-budget counts. End-to-end latency can span multiple refresh intervals;
exceeding 8.33 ms does not itself establish a dropped frame. Input cadence is
measured rather than inferred from the requested 120 Hz. Review request outcomes
alongside successful samples. Report recurring spikes or missing coverage even
when integrity checks pass; the script's zero exit status establishes valid
measurement, not universal performance health.

```sh
uv run --locked python tools/benchmarks/properties-trace-report.py target/properties-run/trace.jsonl --output target/properties-run
uv run --locked python tools/test_tools.py -p 'test*trace_report.py'
```

## Property editing chain benchmark

This benchmark locates synchronous editing costs before changing SDK or editor
behavior. It uses the production `Editor` entry points, actual imported model
data, isolated edit candidates, and the CPU scene/evaluation path. It does not
measure GPUI layout, GPU work, native event delivery, FPS or input-to-photon
latency. Native Properties probes below check the ownership behavior separately.

### Run

```sh
uv run --locked python tools/benchmarks/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/property-chain-unique \
  --reader-policy unique
uv run --locked python tools/benchmarks/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/property-chain-retained \
  --reader-policy retained
```

Requires the local model and the SDK sources in this workspace; no sibling
checkout is needed. A saved
`project.zipzi.json` can also be supplied. Missing resources are explicitly
reported as skips; a missing model, failed edit or incomplete run is an error.
Output directories must be new. Source model files are never saved or modified.

Defaults: three fresh processes per build, 10 warmup interactions and 60
measured interactions per case, with 12 updates per drag gesture. Each drag
alternates two distinct values. Half the gestures finish and undo, half cancel.
Thus each drag case has 720 measured updates per process and 30 finish/cancel
samples each. `--samples`, `--warmup`, `--updates`, `--rounds` and substring
`--filter` control the workload. Small smoke runs are useful for validation but
do not establish stable performance or tail latency.

The runner first builds and copies two Release binaries:

- Plain: no `bench-instrumentation` feature, so internal clocks and counters
  compile out. These timings determine the total editing cost.
- Instrumented: nested thread-local spans in editor/core/SDK. Records both
  inclusive and self time, and calls per operation. These timings attribute
  work and identify repetition. They are not substituted for plain totals.

All builds finish before measurement. Binary order alternates between rounds.
The runner rejects concurrent Cargo/rustc activity detected at run boundaries,
source changes, changed model files, incomplete coverage, duplicate samples,
unbalanced timing totals and mismatched build modes. Boundary checks cannot
exclude a short-lived unrelated workload between checks; keep the machine idle.
Thermal state, hardware, compiler version, relevant build environment, repository
revisions/diffs, source hashes, full source archive, model-subtree hashes and
binary hashes are retained. No model assets are copied into the report.

### Workload and timing boundaries

Mesh selection prefers editable ArtMesh215 when present; otherwise it chooses
the median-size mesh editable at the default parameter pose. Its vertex count
and actual Object/MeshKeyform scope are reported. The largest motion by track
count and largest expression by parameter count are selected. Model geometry,
parameter counts, motion track/key counts and import diagnostics are reported.
Model-specific results must not be generalized to all meshes or long clips.

| Workload | Timed operations |
| --- | --- |
| Property reads | Mesh, Motion and Expression inspector values |
| Commits | Mesh name/opacity/multiply, Parameter default, Motion/Expression fade, Physics node mobility |
| Property drags | Mesh opacity/multiply, Parameter default, Motion/Expression fade, Physics mobility |
| Timeline | Write an existing parameter key, with a retained previous Timeline projection |

`commit` covers `Editor::commit_properties`, including outer prevalidation,
history publication and synchronous post-edit invalidation. `after_commit`
consumes the panel index and CPU preview. Gesture `begin`, each `update`,
`preview`, `finish`/`cancel`, and `after_finish`/`after_cancel` are separate.
Combined totals sum paired phases of the **same input** before computing
percentiles; the report never adds independent P95s.

Import, texture decoding, viewport setup, baseline patch construction, assertions,
undo restoration, report allocation and I/O are outside the timed operations.
Ordinary preview evaluation and scene creation remain inside their stated
phases. Animation uses Editing mode, not physics simulation or Result playback;
Physics mobility here measures configuration authoring, not simulation speed.
The benchmark evaluates each update synchronously, without background-worker
coalescing. This is a cost decomposition, not an input-delivery simulation.

Every measured update must change its candidate value and leave history intact.
Mesh opacity/multiply are checked against actual evaluated drawables, accounting
for inherited appearance and stored-keyform scope. Resource/Parameter values
and Timeline projections are checked directly. Every commit/finish must publish
one history step; undo and cancellation must restore the original values.
CPU Modeling/Animation results must pass production acceptance checks. A frame
accepted here has not been GPU rendered or displayed by an OS window.

### Ownership controls

`unique` does not retain an extra `ObjectIndex` reader across the edit. The
document cache can still use `Arc::make_mut`, but usually owns its only strong
reference. `retained` holds the previous immutable index through the editing
and preview phases, forcing copy-on-write when a local update preserves that
old snapshot. This is a deliberate ownership control, not an assertion that
every UI input has an outstanding reader. The `editor.object_index_unique` and
`editor.object_index_cow_clone` stages distinguish actual execution.

Both modes keep normal production scene/animation state and undo snapshots.
Only the additional panel index reader changes. Timeline always retains the
previous projection, independently of this panel-index control.

The current index shares slot maps, dependency paths, bindings and draw groups.
Object and relation payloads use pages of 32 shared records. A retained reader
therefore causes a shallow copy of the page directory and touched pointer page,
then a copy of each changed record. Unchanged records retain their identity;
stored-keyform appearance edits can leave every base-object record shared.
`editor.object_index_cow_clone` now measures the small index-root copy, while
`editor.index_record_cow_clone` measures detached records. Older results predate
this storage layout and measured the full index clone in the former stage.

Inspector descriptors declare preview capability, write backend and projection
effects separately. Enabling a control's preview does not select scalar writes
or permit structural-cache reuse. Runtime evaluation invalidation remains a
core editing rule. Motion tracks and events are shared sequences: Fade patches
validate their descriptor fields, while Duration/FPS patches additionally check
track endpoints and event bounds. SDK publication still validates the complete
document; these changes reduce repeated candidate work within a gesture.

Check actual native ownership with the existing window harness:

```sh
ZIPZI_BENCH_CHAIN_STAGES=1 ZIPZI_BENCH_PROPERTY_CASE=opacity_drag \
  ZIPZI_BENCH_WARMUP=10 \
  ./tools/benchmarks/properties-benchmark.sh target/property-chain-native-opacity 60 120
```

The feature-enabled native harness records stage counts/times on the UI thread
for each existing `properties_tick`, then emits them outside the input timing
scope. Background worker spans are deliberately not captured. Events use stage
names such as `core.fork_candidate` and `editor.object_index_unique`, share the
tick's ID, and retain warmup association. Use this run to check which headless
ownership regime applies; native CPU and GPU latency continue to come from the
existing Properties report. Do not use instrumented native timings as an
uninstrumented before/after comparison.

### Read the report

`summary.md` lists the median of the three per-process medians, their range,
the median of per-process empirical P95s, and the paired probe/plain median
ratio. It also lists all stages' average calls, mean self time, self share of
phase wall time and inclusive time. `summary.json` retains each process's
statistics, call-count ranges, skips and metadata. Raw per-input values and
stages remain in `round-*-*.jsonl.gz`.

Self times partition the outer root span exactly. Inclusive parent/child times
overlap and must not be added. A low-level copy or validation span is often
nested inside a fork, SDK edit, or editor operation. Memory allocation/free
and scheduling pauses are included in the synchronous wall time; these are
not thread-CPU counters. No allocation-byte or RSS claim is made.

For example, compare `core.fork_candidate` calls before deciding to reuse
candidates, `core.checkpoint_clone` versus `core.checkpoint_swap_indexes` for
copy/index costs, `core.prepare_evaluation` versus `core.evaluate` for cold
preparation, and `core.validate_structure` versus `core.validate_motion` for
transaction validation. Compare index ownership stages before attributing
local-refresh cost solely to the changed object. Large uninstrumented residuals
require additional probes before drawing a refactor conclusion.

Probe overhead matters especially for microsecond reads and Physics patches.
Check repeated plain/probe ratios and run-to-run variation. P95s from 30 finish
samples describe those samples; they are not a robust rare-tail estimate.
Call-count assertions and value/history checks are generally more stable than
millisecond thresholds.

For supplemental macOS call stacks, use the saved plain binary after all timed
runs finish. These stack samples include assertions and untimed restoration, so
do not mix their timings or sample shares into the baseline statistics:

```sh
target/property-chain-unique/property-chain-plain \
  models/local/mao/runtime/mao_pro.model3.json 300 10 12 mesh_opacity_drag unique \
  > target/chain-stack-inputs.jsonl &
chain_profile_pid=$!
sleep 1
/usr/bin/sample "$chain_profile_pid" 5 1 -file target/chain-stacks.txt
wait "$chain_profile_pid"
```

### Refactor comparison

The runner preserves plain binaries and fingerprints. See the [refactor comparison](../../docs/archive/benchmarks/2026-10-21-property-chain-refactor.md) for historical before/after measurements across property policies and index snapshot changes.

## Runtime Editor pairs

R2 uses copied Release executables so compilation never overlaps measurements.
`tools/runtime/editor_benchmark_pairs.py` expects both `baseline-` and `candidate-`
prefixes for `property-chain` (the `property_chain_benchmark` example), `window`
(the `zipzi-editor` binary with `test-support`), `timeline` (`timeline_acceptance`)
and `interaction` (`runtime_interaction_acceptance`). Build each isolated source
version before timing and record source, binary and model hashes. When sharing a
Cargo target directory across source roots, clear the owned package artifacts
between version builds to avoid stale relative dep-info; retain copied binaries.
Each measured process receives a private temporary Runtime directory/profile and
no inherited owner fd, so a resident desktop host cannot intercept its startup.
The short temporary endpoint also avoids Unix socket limits in long output paths.
The first executable alternates by pair (AB, BA), and that order is part of the
resume identity, preventing systematic first/second-position bias.

```sh
python3 tools/runtime/editor_benchmark_pairs.py \
  --binaries target/runtime-r2 --output target/runtime-r2/paired \
  --model models/local/mao/runtime/mao_pro.model3.json \
  --warp 5101be9b-3d35-45a7-8764-b8d62616ada4 --pairs 10 \
  cpu properties color deformations animation timeline interaction
python3 tools/runtime/editor_benchmark_report.py target/runtime-r2/paired \
  --output target/runtime-r2/comparison.json --pairs 10
```

The runner defaults to five alternating fresh process pairs per scene; the R2
record uses ten (`--pairs 10`) and retains all ten pairs. CPU runs have 60
interactions, 10 warmups and 12 updates per gesture; native Properties runs its
Mesh opacity numeric drag and a separate RGB continuous color drag with 30 samples and 10 warmups; deformation uses 120 samples
and playback runs for 5 seconds. Timeline and dual viewport/background-client
probes use 60 measured samples. It checks existing reporter completeness gates,
records `/usr/bin/time -l`, and fingerprints executables/models for safe resume.
It needs macOS, an unlocked native desktop, Metal and the local model. It does not
relax existing timing limits or accept incomplete native traces.

For R5 comparisons, `--read-batches` adds continuous CPU, independent Preview and
GPU observation cases to `interaction`. Copy the same interaction driver and its
`background.rs` module into both source versions before building. Each case warms
one batch, continues four-target batches during the pointer loop, and cancels and
joins its worker afterward. Reports retain raw input samples and background
completion/worker-step counts; startup work alone does not satisfy overlap.
Runtime admission and completion stay on the UI thread; workers execute Work
without touching RuntimeHost or retaining its locks.
These test-window timings end at a current Metal draw and exclude the compositor;
they do not replace real-window animation playback measurements.

The interaction driver alternates native pointer gestures between two Metal
surfaces, verifies actual panning and current displayed frames, then repeats while
a different Runtime client publishes another model on a worker. The R1 control
adapter runs identical SDK rename/commit work on a separately owned SDK session;
R2 uses the same Runtime as the visible views. It checks visible history remains
unchanged. These headless input-to-current-draw figures exclude OS input delivery,
screenshot readback and compositor presentation; real-window reporters retain
their separate correlated presentation boundary.

The report keeps each operation separate, including per-process P50/P95 values,
their medians and ranges. Allowed increases are the larger of P50 10% / P95 15%
and 0.2 ms CPU / 1 ms input-to-display. A case or timing boundary missing from one
side fails comparison. No pooled average can mask a slow case.

For Mao, the explicit Warp matches the audited editable target in the R0/native
baselines. The default largest Warp has a singular parent deformation and rejects
the gesture even in R1. Preserve a rejected trace separately; never report it as a
low-latency pass. Choose an audited editable target when using a different model.

### Windows shared texture presentation

Windows mesh overlays now mirror the Metal edge/vertex shader passes using
D3D11-native immutable surfaces. For a same-binary GPUI comparison set
`ZIPZI_TRACE_BATCH_MESH_EDGES=1`; leave it unset for the native shader path.
The switch retains full overlays and is recorded as
`diagnostics.native_mesh_overlay` (and the historical `metal_mesh_overlay`).
`overlay_shader_surface.fields.pass=1|2` identifies the edge/mesh-marker surfaces;
it is independent of `paint_shared_surface`, which identifies the model preview.
Detailed traces should have no `overlay.gpui_mesh_edges` on the native path;
`overlay.gpui_markers` still includes non-mesh controls, so its presence alone is
not a shader fallback. Compare platform CPU submission as well as UI paint and
new-image intervals; lower primitive counts do not establish screen latency.

Windows previews now publish `shared_texture_submitted` and paint through
`paint_shared_surface`, without the former preview `readback`/`compose_image`
sequence. `windows-gpui-trace-report.py` joins both generations to CPU platform
returns. Historical metric keys containing `image` also cover shared surfaces.
A publish timestamp on the new path is an asynchronous submission boundary;
it no longer implies that model GPU execution has completed. Do not interpret
publish-to-paint or platform-return metrics as screen latency. Image atlas events
may still occur for unrelated UI icons and thumbnails.
