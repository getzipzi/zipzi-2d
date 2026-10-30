# Windows Modeling parameter preview chain — 2026-10-30

The GPUI Kit base vendoring and DockArea bounds patch from this experiment were
subsequently reverted at the user's request. The current source uses the original
pinned Git base package. The captured binaries/results below describe the original
experiment, including that patch, and are not post-revert measurements.

## Scope and acceptance boundary

Compare the actual Modeling SlideNumberInput and linked two-axis slide pad against
the actual Animation Timeline parameter SlideNumberInput. Keep the full Modeling
viewport, toolbar, edit overlays and callbacks. No navigation-only viewport or
hidden overlays are used for these results.

Use UI draw cost, outer input dispatch/effect-flush cost, input to the associated
CPU platform draw return, and the intervals between those new-image returns.
Intervals above 25 ms distinguish missed 60 Hz opportunities from ordinary
16.67 ms jitter. These measurements exclude OS input delivery, DWM and scanout;
they are not physical screen latency or PresentMon display FPS.

## Changes

- Permit ordered completed parameter-preview poses during a continuous preview
  run. Request order, scene ceiling, binding, metrics and presentation floor are
  still checked. Authoring changes end the run; an intermediate pose cannot start
  a new editing gesture. This prevents rejection starvation but did not by itself
  improve frame-interval P95 in the eight retained progressive-only measurements.
- The measured experiment skipped unchanged DockArea bounds writes by patching
  the pinned GPUI Kit base package. That dependency change has since been reverted;
  its source remains only in the saved experiment artifacts.
- Only write Properties visibility when it changes. Status feedback synchronizes
  its independent StatusBar without notifying the Editor owner.
- Reconcile parameter availability on owner notifications, update row availability
  only on transitions, and construct virtual-list entries from immutable row/header
  descriptions rather than mutating the panel during prepaint. This avoids
  subscribing the whole list to individual changing thumb values.
- Exercise the actual number and linked-pad inputs in the native driver, verify
  active drag state, mesh deformation, both pad axes and unchanged preview history.
  Add input-sync, interval P99 and >25 ms counts to the Windows report.

## Fixture and method

Mao source: `models/local/mao/runtime/mao_pro.model3.json`. Modeling selects all
150 visible unlocked meshes, retaining 5,159 vertices and 12,202 edges. Both
controls deform 93 meshes in the displayed result. Number changes one parameter;
pad changes two. The pad link is created only in each isolated imported document
before warmup. Optimized audits verify history stays `[0, 0]` throughout the drag.
No user project is saved.

Each fresh Release process uses a separate Runtime/configuration directory, 30
warmup inputs and 180 measured inputs. Target frequency is 120 Hz, but the Windows
benchmark timer delivers roughly 16 ms input spacing. This is a limitation of
these captures; they do not establish behavior at a real 120 Hz input rate.
Detailed stage probes are kept separate from the final non-detailed comparisons.
Compilation must not overlap any measured process.

The baseline binary includes the same actual-control harness and full Modeling
presentation before this shared-chain optimization. The final comparison uses
alternating AB/BA order for each input, with three Timeline controls interleaved.
Report medians of per-process percentiles, not pooled samples.

## Results

| Input | Runs | UI draw P50 (ms) | Input sync P50 (ms) | Input → CPU return P50 (ms) | New-image interval P95 / P99 (ms) | Intervals >25 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Number before | 2 | 9.11 | 2.30 | 29.80 | 35.22 / 37.09 | 24.4% |
| Number after | 2 | 6.83 | 2.30 | 27.10 | 34.36 / 35.67 | 16.2% |
| Pad before | 2 | 9.06 | 2.34 | 30.58 | 35.28 / 36.75 | 23.7% |
| Pad after | 2 | 6.71 | 2.28 | 26.72 | 34.52 / 35.78 | 14.4% |
| Timeline | 3 | 4.92 | 1.03 | 23.36 | 20.09 / 33.53 | 1.7% |

UI P50 improves by 25.1% for number and 25.9% for pad. The >25 ms interval
fraction drops from 24.4% to 16.2% for number and from 23.7% to 14.4% for pad.
Input sync remains about 2.3 ms. Interval P95 remains around 34–35 ms, compared
with Timeline's 20.09 ms and 1.7% long intervals. The Timeline continuity target
has **not** been reached. The UI and some long-interval reductions are useful,
but this is not a claim that Modeling now feels as responsive as Timeline.

The respective before/after per-process UI P50 ranges are 8.98–9.25 /
6.82–6.84 ms (number) and 9.04–9.09 / 6.66–6.77 ms (pad). Timeline UI P50 is
4.88–4.98 ms. Measured input-interval P50 is 16.07–17.29 ms across these runs.
This timer/dispatch contention difference is retained in the results rather
than relabeled as identical actual 120 Hz input.

Baseline binary SHA-256: `94ff1305a134b6171c9a0d05b9c5142b70bd2a1c1929521180ef0500a2504cb4`.
Candidate SHA-256: `33468481cb3a67d16dee084d2ef7247c057a686b40fef5cc06b000c34eddb834`.
Model JSON SHA-256: `27b71a0e5476d870ba69d1b52f5bd068ed5a03113739b4f5337e579e954b7e2a`.
Both binaries are based on `a4420a78` plus retained uncommitted source snapshots;
the candidate uses the normal root Cargo patches, without command-line dependency
overrides. Final measurements contain 8 Modeling processes and 3 interleaved
Timeline processes; both controls verify actual image updates, and the candidate
audits verify one/two changed parameters and unchanged history.


The early combined detailed probe reduced UI P50 from approximately 8.7 ms to
6.45–6.51 ms, with no Editor root render scopes in either measured gesture. It
still had interval P95 of 34.34–34.61 ms. Fewer root builds and lower UI cost alone
are not evidence of Timeline-like continuous presentation.

A separate detailed Modeling number probe attributes approximately 0.99 ms to
`core.evaluate` inside the command, and 0.87 ms to `editor.scene` while creating
the preview request (including 0.83 ms canvas geometry build). These nested
percentiles must not be added. HostedSession preview changes are local and do not
publish a Runtime change echo. A Runtime synchronization echo was investigated
and excluded for this path.

The worker/render scheduling split remains relevant: in the combined number
probe, request-to-publish P50 was 4.13 ms, publish-to-UI-receive 3.77 ms and
accept-to-image-paint 11.77 ms. In the earlier Timeline control, publish-to-receive
was 0.08 ms. These are separate distributions including setup, used to guide
investigation; they are not a sum or a controlled claim of causal attribution.

## Validation and reproduction

Release build, 14 `ui_performance`, 23 sorting, 6 SlideNumberInput, 19 Dock tests
and 9 report tests passed, with rustfmt and diff whitespace checks. The 29 focused
canvas tests also passed, including ordered parameter poses, stale hit-test
guards and resize/document rejection; their output is in `canvas-tests.log`.

The serial parameter group has **49 passes and 2 failures**, not a full pass:
`parameter_enter_applies_drafts_and_preserves_unedited_precision` retains 0.606
instead of 0.6, and `parameter_indirect_button_selects_the_nearest_bound_ancestor`
has a different parameter focus. Both failures reproduce in the older
`zipzi_editor-74c84b06ddacbea1.exe` (623 tests, built before the panel/status/cache
changes); logs are `old-number-test.log` and `old-indirect-test.log`. They are
retained without relaxing assertions or changing unrelated behavior. A group
deletion test also failed in the first parallel group invocation, then passed
both individually and in the serial group; the initial failure remains recorded.


All raw traces, native logs, audits, executable hashes, before/final source
patches, the dependency experiment and rejected/non-improving experiments remain
under `target/windows-parameter-chain-20261030/`. Timeline controls remain under
`target/windows-timeline-number-20261030/timeline-current-*/`.

Build the candidate from the repository root:

```powershell
cargo build --locked --release -p zipzi-editor --features test-support,bench-instrumentation
```

The saved `run.py` accepts fresh names, e.g. `baseline-number-repeat-1` versus
`optimized-number-repeat-1`, and `baseline-pad-repeat-1` versus
`optimized-pad-repeat-1`. Names beginning with `baseline` select `before.exe`;
other names select `after.exe`. A name containing `number` selects the number
input, otherwise the pad. `final_runs.py` records the complete interleaved order.
Use new output names; runners refuse to overwrite existing evidence.
