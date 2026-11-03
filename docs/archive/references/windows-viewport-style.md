# Windows viewport presentation comparison (2026-10-30)

On commit `a4420a78` plus this experiment, the Modeling parameter-drag benchmark becomes faster when its viewport uses Animation presentation. This isolates presentation costs; it does not switch the core viewport identity or execute the Timeline/Physics input pipelines.

## Controlled change

`ZIPZI_BENCH_VIEWPORT_STYLE=animation` uses the same navigation content factory as Animation: model image, empty edit overlay, hand navigation, no deformer controls, navigation toolbar, no Modeling tool callback or object menu. The default `modeling` retains normal editing. The core binding stays Modeling so the same model worker, frame provenance, parameter slider and selection remain valid. A bare `ViewportKind::Animation` substitution would require motion content and reject Modeling frames.

## Native results

One saved Release binary (`test-support,bench-instrumentation`), SHA-256 `dec04e5652ade77ae081787889031734049bf7dac5b131afe9de42a6bb2c8b83`. Mao full selection, 30 warmups + 180 measured inputs, target 120 Hz. Each run independently audits 150 selected Meshes, 5,159 vertices, 12,202 edges and 93 Meshes whose positions changed. Request textures remain 1061 × 1478 in all ten runs. Every run completed with zero lost events, render errors and preview readbacks, with shared texture submissions and paints present. No build overlapped timing.

Detailed runs use Modeling → Animation style → Modeling paint disabled → Modeling paint disabled → Animation style → Modeling. The last four runs disable detailed recording and use Modeling → Animation style → Animation style → Modeling.

| Run | UI draw P50 / P95 (ms) | Input → first associated platform return P50 / P95 (ms) | Associated inputs / 180 | Modeling render overlay builds |
| --- | --- | --- | --- | --- |
| baseline-detail-1 | 8.82 / 9.41 | 28.72 / 32.87 | 168 | 168 |
| animation-detail-1 | 6.75 / 7.55 | 25.01 / 28.61 | 159 | 0 |
| no-paint-detail-1 | 7.29 / 8.00 | 26.37 / 30.24 | 166 | 167 |
| no-paint-detail-2 | 7.58 / 8.20 | 26.43 / 30.06 | 165 | 166 |
| animation-detail-2 | 6.86 / 7.62 | 25.68 / 28.48 | 162 | 0 |
| baseline-detail-2 | 8.87 / 9.89 | 29.07 / 32.19 | 172 | 172 |
| baseline-main-1 | 8.73 / 9.45 | 28.73 / 32.69 | 168 | not instrumented |
| animation-main-1 | 6.99 / 7.83 | 25.99 / 29.79 | 161 | not instrumented |
| animation-main-2 | 6.83 / 7.53 | 24.76 / 29.22 | 162 | not instrumented |
| baseline-main-2 | 8.67 / 9.57 | 28.70 / 32.45 | 168 | not instrumented |

## Interpretation

Without detailed recording, the mean of the two per-run UI P50 values decreases by 20.6%; the mean associated-input P50 decreases by 11.6%. These are descriptive paired-run results, not a statistical guarantee for every input. Faster cases coalesce more inputs; the table retains association counts. CPU platform return is not screen presentation, and no GPU timestamp/PresentMon measurement was collected.

Only skipping paint still builds the Modeling overlay 166–167 times and leaves prepaint P50 at 2.33–2.39 ms. Animation presentation builds it zero times and reduces prepaint to 1.79–1.82 ms, with paint at 1.29–1.33 ms versus Modeling 2.61–2.66 ms. This supports a contribution from both overlay drawing and the additional Modeling presentation work. This experiment does not separate overlay construction from toolbar/control layout individually. The actual Animation/Physics worker and input paths may contribute further differences and were not benchmarked here.

The practical next optimization is to reduce Modeling overlay/control work while preserving editing functionality. Permanently replacing it with navigation-only presentation would remove editing handles and feedback.

## Validation and reproduction

12 focused `ui_performance::` tests, the Release build, rustfmt checks for the four changed Rust modules, and the ten native runs passed. The animation-style detailed runs also assert zero origin-1 Modeling overlay builds, so a skipped image update cannot count as a successful result.

Binary, local runner, raw traces, source patch, audits and `summary.json` are retained in `target/windows-viewport-style-20261030/`. Run the saved runner with fresh output names to reproduce:

```powershell
uv run --locked python target/windows-viewport-style-20261030/run.py --detail --order baseline-detail-3 animation-detail-3 no-paint-detail-3 no-paint-detail-4 animation-detail-4 baseline-detail-4
uv run --locked python target/windows-viewport-style-20261030/run.py --order baseline-main-3 animation-main-3 animation-main-4 baseline-main-4
```
