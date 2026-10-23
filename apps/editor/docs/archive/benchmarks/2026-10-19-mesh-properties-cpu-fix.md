# Mesh properties CPU fixes — 2026-10-19

The redundant index/topology rebuilds and swatch tooltip whole-window refresh
have been removed. The three priority commits now take roughly half the CPU
time. This is an improvement, **not an all-clear against the 120 Hz budget**:
complete synchronous dispatch and some UI draws still exceed 8.33 ms.

## Method

Same release build configuration, native GPUI window, Metal compositor, Mao
260-mesh model, selected ArtMesh215 (158 vertices), and 120 Hz target as the
[original benchmark](2026-10-19-mesh-properties.md). Each case uses **one warmup
and three measured interactions**. Values are checked after every interaction;
edits are restored without saving the source model.

Four focused before/after runs provide the paired comparison. A separate final
33-case suite checked all 99 measured interactions. Both sets have empty trace
issue lists, checked values, and native presentations. The tooltip case now
alternates Original/Current on short runs and dwells 600 ms, so three samples
actually exercise tooltip display and switching. Both paired builds use this
same workload.

Machine: Apple M4, 16 GiB RAM, 10-core GPU, external 4K/120 Hz display. No source
edits or test/compiler workloads ran during the final native measurements.

## Changes

- Swatches use the existing managed tooltip overlay. Showing/hiding a tooltip
  notifies that overlay instead of invoking GPUI's native whole-window refresh.
  Tooltip content, placement and show/hide behavior remain covered by UI tests.
- Name, Enabled and Opacity commits refresh the changed mesh's ObjectIndex
  payload. The relation graph and binding/influence index are preserved using
  copy-on-write; other property changes retain the full rebuild path.
- Name updates the canvas label index while sharing unchanged topology and
  picking geometry. Opacity/Enabled also reuse geometry and publish fresh
  appearance/visibility with the new frame. Old presented snapshots remain
  immutable.
- Inspector reads use only inspector fields, avoiding serialization of vertex
  arrays. These three writes clone the typed mesh instead of converting its
  geometry through JSON. Scope, stale-baseline, lock, transaction and SDK
  structural validation checks are retained.
- Optional trace stages distinguish property commit CPU/wall time and immediate
  redraw CPU/wall time. Clocks are not read when tracing is disabled.

## Paired results

Milliseconds. Commit/redraw values are the **maximum of three thread-CPU
measurements**, not a statistically stable tail estimate. Dispatch is the maximum
native event wall time. Tooltip P95 is over UI draws during the three hover
interactions, including managed show/hide transitions.

| Case | Commit CPU before → after | Immediate redraw CPU before → after | Dispatch wall before → after |
| --- | ---: | ---: | ---: |
| Name | 15.58 → 7.58 | 3.43 → 0.76 | 23.13 → 13.35 |
| Opacity text/Enter | 14.63 → 8.14 | 3.07 → 0.81 | 21.78 → 13.67 |
| Enable | 14.78 → 7.73 | 3.05 → 0.99 | 17.81 → 8.78 |

Swatch tooltip UI draw P95: **22.43 → 5.99 ms** (73% lower). Maximum draw:
22.43 → 10.90 ms; draws over 8.33 ms: 6/7 → 2/78. The managed tooltip animates,
so it produces more small draws than the old popup; this is not a claim of lower
total CPU over the entire hover dwell. The separate complete-suite run measured
4.58 ms P95 and 8.331 ms maximum for this case.

The initial stage investigation found about 5–6 ms in whole ObjectIndex rebuilds
and about 4–5 ms in SDK commit work on Mao. Eliminating the index rebuild does
not eliminate the SDK's complete-document validation/history publication.

Keyboard dispatch can also draw a pending UI frame before processing the key.
Those draws remain included in native-event wall measurements; they must not be
confused with the separately measured commit CPU stage. Scripted hover/debounce
waits are excluded from native-event metrics.

## Complete-suite verification and limits

All 33 cases completed with three checked inputs each, including runtimeId, Part,
masks, both Opacity paths, Multiply Color, blend, Enable, picker open/close,
RGB/HSV tabs and text/drag inputs, hue/SV surfaces, invalid hex, Done/Cancel,
reset, swatches and idle.

No GPU execution/delivery errors or unknown preview terminal outcomes occurred.
The largest composition GPU P95 across cases was 8.23 ms. This small sample
does not establish absence of rare GPU stalls.

Remaining CPU budget overruns are visible in the raw results. In the complete
suite, Name/Opacity/Enable native-event maxima were 12.94/13.85/8.82 ms. UI draw
P95 still exceeded 8.33 ms for runtimeId, Opacity drag, inline Multiply hex,
picker open/close, picker hex, RGB blue text, Done and reset (about 8.40–10.36 ms).
These results do not support claiming that every control sustains 120 Hz.

## Regression checks

- 22 editor-core unit tests, including immutable index snapshots, fresh-build
  equivalence, labels/visibility, geometry sharing and Undo/Redo.
- 26 property integration tests, including keyform scope, grouped baselines,
  locks, invalid writes and identity/reference preservation.
- 12 color picker tests, including actual Original/Current tooltip display/hide
  without invalidating an unrelated cached view or emitting edit events.
- 4 existing UI work-count regression tests.
- 26 Python trace-reporter tests, including warmup exclusion and separate stage
  CPU/wall measurements; formatting and diff whitespace checks.

## Reproduction and evidence

```sh
KASANE_BENCH_WARMUP=1 ./scripts/properties-benchmark.sh target/cpu-final-all 3 120
KASANE_BENCH_PROPERTY_CASE=name KASANE_BENCH_WARMUP=1 \
  ./scripts/properties-benchmark.sh target/cpu-final-name 3 120
```

Paired sources: `target/cpu-before-{name,opacity_text,enabled,picker_swatches}`
and `target/cpu-final-{name,opacity_text,enabled,picker_swatches}`. The completed
full suite used `target/cpu-final-all`. These are historical source identifiers:
raw traces, temporary configurations, logs and source snapshots were cleaned at
the user's request after preserving the paired and complete-suite summaries,
environment and SDK revision in the saved JSON. The per-request raw audit is
not retained; outcome counts and trace issue lists remain in those summaries.

The intermediate `cpu-first-fix-name` run failed to build because Cargo reused
an old vendored component artifact; it contains no valid measurement. A targeted
component release rebuild resolved that. Intermediate `cpu-fixed-*` runs are
excluded from this paired comparison; one overlapped compilation/testing.

[Saved paired and complete-suite results](2026-10-19-mesh-properties-cpu-fix.json).
