# Manual Mesh continuous drag, 2026-10-29

Baseline: `042ea6a9`, with the same manual-drag benchmark driver added to both
binaries. Apple M4, 16 GiB RAM, macOS 27.0.1, arm64, Release builds.
[Measurements and hashes](2026-10-29-manual-mesh-drag.json) retain individual
rounds, fixture identities, model/binary hashes and trace locations. Proprietary
model assets are local inputs and are not included in this report.

## Changes

- A private ManualMeshPlan owns the fully validated Core candidate and its change
  receipt. Application checks the exact publication snapshot and installs the
  result, retaining the stale-plan guard and atomic error behavior.
- Frozen source topology admission and its memory estimate are reused. A repeated
  move performs one final joint topology/dependency validation. The public pure
  geometry API continues validating its output.
- When vertex slots are unchanged, each stored channel is cloned once and only
  resampled slots are patched. All sample values are computed before writing.
- Manual previews retain the object/relationship index, updating only mesh counts
  when necessary. Connectivity is compared with the last displayed candidate,
  including meshing constraints, before invalidating the canvas topology cache.

No target UVs, parent/Glue inverse rules, shape sampling, lock checks, history or
rendering quality were relaxed. Conversion budgets remain conservative.

## CPU probes

Three alternating before/after rounds ran from saved executables without concurrent
compilation. Table values are medians of the three per-round P50 measurements;
independent stages are not added to claim an end-to-end percentile.

| Workload / metric | Before | After |
| --- | ---: | ---: |
| SDK: 1,089 vertices, 2,048 faces, 58 stored channels, plan + apply | 23.138 ms | 10.795 ms |
| SDK cumulative allocated bytes / update | 19,903,905 | 7,057,168 |
| SDK allocation requests / update | 28,186 | 7,733 |
| Mao ArtMesh215, 158 vertices: gesture input | 9.358 ms | 4.294 ms |
| Same mesh: panel index + evaluation + CPU preview/overlay | 15.206 ms | 1.009 ms |

The SDK before-round P95s were 24.170 / 23.581 / 43.498 ms; after-round P95s were
11.481 / 11.244 / 11.266 ms. The long-tail baseline round is retained. Allocation
figures count cumulative allocator requests, not peak RSS or GPU memory.
The CPU probe validates every input changes a candidate and that cancellation
preserves the original mesh and history. It does not measure window latency/FPS.

## Native window

The unchanged driver selects the same 90-vertex mesh and vertex 66, follows the
same interior-triangle trajectory, uses a 941 x 1312 preview target and requests
120 Hz input. Each run has 20 warmup and 120 measured inputs. Every measured input
is verified to produce candidate data and an evaluation; each report has 119
correlated presentation timestamps. Both final reports have no issues or lost
trace events. The gesture is committed then undone; no model is saved.

Final pair: `before-repeat` versus `after-verified`.

| Metric | Before | After |
| --- | ---: | ---: |
| New-preview presentation FPS | 43.975 | 81.849 |
| Input handler P50 | 11.115 ms | 6.283 ms |
| CPU scene P50 | 2.052 ms | 1.070 ms |
| Input to correlated presentation P50 | 47.050 ms | 46.786 ms |
| Input to correlated presentation P95 | 80.250 ms | 49.159 ms |

An earlier complete pair measured 43.975 to 72.989 new-preview FPS and P95
87.785 to 49.095 ms. Its optimized build predates cold-path guard cleanup; the
final pair uses the final build. Median presentation latency stayed approximately
constant while update throughput and tail latency improved.

Not every stage improved: native UI draw P50 increased from 1.897 to 2.508 ms.
GPU preview P50 varied from 0.849 to 0.176 ms; no GPU mechanism was changed, so
this variation is not treated as an independently established GPU optimization.
These numbers begin at GPUI dispatch and end at correlated Metal presentation;
they exclude OS input delivery and input-to-photon. They are not a frame-rate
guarantee for other meshes or machines.

Excluded runs are recorded in JSON: an initial driver used a rejected arbitrary
motion, an early trace declared the wrong workload list, and a later repeat timed
out while the Mac was locked. After unlocking, the complete final pair above was
rerun. None of those failed/incomplete runs contributes to the table.

## Verification

- SDK manual mesh / mesh editing / scene content, with `bench-instrumentation`:
  62 passed. The new work-count assertion requires one topology validation per
  repeated plan and none on application of that checked plan.
- Editor Core `mesh_generation::`: 42 passed. The new cache regression checks real
  candidate UV changes, panel snapshot sharing, rendered connectivity consistency,
  topology Arc reuse, insertion counts and cancellation.
- Runtime authoring + attached upload: 11 + 4 passed.
- Native Editor `manual_mesh`: 16 UI + 1 Core passed. The render target with zero
  matches is not counted as coverage.
- `git diff --check`: passed. Full SDK/Editor suites were not rerun.

Reproduction commands are in the [benchmark guide](../../../tools/benchmarks/README.md#manual-mesh-drag).
