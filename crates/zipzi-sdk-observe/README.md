# zipzi-sdk-observe

Captures immutable SDK session snapshots and renders previews with WGPU.

The native backend is Metal on macOS and WGPU elsewhere. Capture evaluates a
detached authoring snapshot; `ResolvedObservation` decodes and fixes actual
texture content once, allowing repeated renders without revisiting live paths.
`capture_many_with_control` checks cancellation between sample validation,
texture decodes and scene digests. An entered decode can finish.

Focused CPU checks:

```sh
cargo test --locked -p zipzi-sdk-observe --lib view::
cargo test --locked -p zipzi-sdk-observe --test capture cancelled_capture
```

GPU observation through the Runtime/Python adapters is tested explicitly with
`./tools/test-sdk.sh python2 --observation`; it requires a usable device. See the
[Runtime/Python observation record](../../docs/plans/sdk-runtime/python2-r5-observation.md).
