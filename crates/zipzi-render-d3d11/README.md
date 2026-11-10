# zipzi-render-d3d11

Native Direct3D 11 rendering for Windows. The renderer consumes the shared
`ScenePlan` and records RGBA8 output on a private deferred context. It supports
legacy and raw blend modes, raw-alpha clipping masks, nested pixel-aligned
offscreens, and premultiplied mipmaps.

Editor hosts supply their D3D11 device and execute each list on its immediate
context before sampling the target. Standalone observation uses
`D3d11Context` to select a hardware device, submit work, and read back captures.
The native library has no GPUI dependency. The wgpu dependency is for GPU
parity tests.

Pose updates reuse a dynamic vertex buffer and retain immutable indices while
topology is unchanged. Each command list uploads its own complete geometry with
`WRITE_DISCARD`, including view-only redraws, so dropping or replaying other lists
cannot change its inputs. Draw constants use a dynamic pool deduplicated within
each recording; the pool retains the last recording's working set, not a history
of parameter values. GPU tests exercise delayed submission, discarded frames,
topology edits and replay after renderer teardown.

An Editor fixture probe separates scene planning, geometry packing, attachment
allocation and command encoding. It precomputes 120 `Angle_X` poses, warms 30
frames, and drains the GPU outside the measured recording/submission intervals:

```powershell
cargo run --locked --release -p zipzi-editor-render --example d3d11_record_benchmark -- models/local/mao/runtime/mao_pro.model3.json 240
```

The CSV is CPU recording evidence, not UI frame time or display latency. Use the
native window parameter benchmark for end-to-end comparisons. The recording
callback also exposes `native_d3d11_plan_ready`, `native_d3d11_layout_ready`,
`native_d3d11_targets_ready` and `native_d3d11_encoded` boundaries.

```powershell
cargo test --locked -p zipzi-render-d3d11 --test parity -- --nocapture
cargo test --locked -p zipzi-observe --test capture
```
