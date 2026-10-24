# Property editing chain benchmark

This benchmark locates synchronous editing costs before changing SDK or editor
behavior. It uses the production `Editor` entry points, actual imported model
data, isolated edit candidates, and the CPU scene/evaluation path. It does not
measure GPUI layout, GPU work, native event delivery, FPS or input-to-photon
latency. Native Properties probes below check the ownership behavior separately.

## Run

```sh
python3 tools/benchmarks/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/property-chain-unique \
  --reader-policy unique
python3 tools/benchmarks/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/property-chain-retained \
  --reader-policy retained
```

Requires the local model and sibling SDK checkout. A saved
`project.kasane.json` can also be supplied. Missing resources are explicitly
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

## Workload and timing boundaries

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

## Ownership controls

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
KASANE_BENCH_CHAIN_STAGES=1 KASANE_BENCH_PROPERTY_CASE=opacity_drag \
  KASANE_BENCH_WARMUP=10 \
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

## Read the report

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

## Refactor comparison

The runner preserves plain binaries and fingerprints. See the [refactor comparison](../archive/benchmarks/2026-10-21-property-chain-refactor.md) for historical before/after measurements across property policies and index snapshot changes.
