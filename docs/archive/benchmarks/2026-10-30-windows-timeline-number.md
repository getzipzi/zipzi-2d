# Windows Animation Timeline parameter baseline (2026-10-30)

This is the reference workload for subsequent Modeling interaction performance comparisons. It uses the actual Animation workspace and Timeline ParamAngleX SlideNumberInput, not the Modeling viewport with an Animation-like presentation.

## Reference measurements

Fresh native Release processes, original local Mao model, the first playable looping motion from its catalog, Editing preview mode, the production Timeline number drag handler. No generated Timeline fixture, UI-only mode or viewport-style override. Each run has 30 actual pointer warmups and 180 measured pointer moves at a target 120 Hz. Detailed animation instrumentation is disabled.

| Run | UI draw P50 / P95 (ms) | Input → associated platform return P50 / P95 (ms) | Associated inputs / 180 |
| --- | --- | --- | --- |
| baseline-1 | 5.26 / 6.10 | 23.83 / 27.88 | 174 |
| baseline-2 | 5.22 / 5.88 | 24.47 / 27.96 | 172 |
| baseline-3 | 5.32 / 5.99 | 23.60 / 27.63 | 171 |

**Reference: median of the three per-run P50s is 5.26 ms UI draw and 23.83 ms input-to-platform return. Median per-run P95s are 5.99 ms and 27.88 ms.** Preserve both distributions and association counts for future comparisons.

The actual input interval P50 is approximately 16.00 ms despite the 120 Hz timer target. These measurements begin at synthetic GPUI dispatch and end at the first CPU platform draw return for the corresponding animation texture. They exclude OS delivery and screen presentation. Some inputs are coalesced; missing associations are reported, not assigned zero latency.

The prior presentation-only comparison had Modeling UI draw P50 8.67–8.73 ms and Animation-style Modeling 6.83–6.99 ms. This real Timeline workload has a different workspace tree, input/candidate chain, camera and viewport dimensions; it is a performance reference, not a controlled attribution of their difference to a single viewport flag.

## Correctness and trace changes

The existing native harness verifies the number control remains sliding, candidate revision advances beyond 1, Escape cancels the candidate and history is unchanged. Completion of every run confirms those checks passed. All three traces have 210 actual drag samples, 30 warmup ticks and 180 measured ticks, zero lost events, zero render errors and zero preview readbacks. New animation textures are submitted and painted. This probe verifies candidate changes and correlated frame delivery; it does not count per-mesh position differences.

`animation_enqueued.input` now records the originating InputScope ID. The Windows reporter follows that identity through evaluation, rendering, shared texture paint and the enclosing CPU platform draw return. It never infers an association from nearby timestamps. A focused regression verifies this join and rejects an input ID of zero. Seven reporter tests pass. The native number/canvas drag harness now honors configured warmup and emits workload-9 benchmark ticks, then allows 250 ms for the final candidate to reach the viewport before cancellation.

## Frozen artifacts and reproduction

Binary SHA-256: `c37a6fc18c6aa9b0e0614e216ca917884fcb90527155f0fc282b6d8f0e9ab167`. The code is based on `a4420a78` plus the saved source patch. Binary, runner, raw traces, per-run JSON, `baseline.json`, source patch and base commit are preserved in `target/windows-timeline-number-20261030/`. No compile overlapped the measurements. The Release build succeeded.

```powershell
uv run --locked python target/windows-timeline-number-20261030/run.py --order baseline-4 baseline-5 baseline-6
uv run --locked python tools/test_tools.py -p test_windows_gpui_trace_report.py
```

The runner starts the saved `comparison.exe` in isolated runtime/config folders. Use new run names; earlier results are not overwritten. The relevant automatic-driver variables are `ZIPZI_BENCH_ANIMATION=1`, `ZIPZI_BENCH_ANIMATION_DRAG=number`, `ZIPZI_BENCH_SAMPLES=180`, `ZIPZI_BENCH_WARMUP=30`, `ZIPZI_BENCH_HZ=120`, plus a fresh `ZIPZI_FRAME_TRACE` and the local Mao model path.
