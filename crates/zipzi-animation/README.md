# zipzi-animation

Deterministic CPU evaluation of motions, expressions, poses and physics.

`MotionPreview` evaluates playback into independently owned frames. It reuses
per-preview scratch storage; concurrent reads of the same preview serialize.
`MotionEditingPreview` samples a raw clip from the default pose without motion
fades, expressions, physics or events, and resolves Pose immediately.
`motion_positions` and `canonical_motion_seek_time` expose playback coordinates
for timeline seeking.

Curve-authoring primitives compile a contiguous Auto Smooth group with zero
endpoint slopes and time-weighted monotone interior slopes, map the Contract's
fixed easing controls, and quantize segment-local sample times for Frame Step.
Resolve the outgoing segment with `CompiledCurve::segment_at` before quantizing;
`sample_motion_segment` evaluates its base curve. Monotone Bezier time inversion
uses a safeguarded f64 solve; non-monotone curves retain the MotionBehavior V2
root choice. `CompiledCurve::from_track(track, fps)` resolves persistent automatic
groups and frame stepping. Its FPS is part of preview cache identity;
`sample_motion_curve_base` exposes the continuous curve for structural edits and
optional graph overlays. Motion fades, mixing, events and physics keep real time.

Motion tracks and events use copy-on-write storage. Edits validate the affected
fields and dependencies; SDK publication validates the complete document.

```sh
cargo test --locked -p zipzi-animation
```

MotionPreview captures immutable DocumentState. Host consumers bind QuerySource
with `bind_source`; that value includes the full publication identity or Draft
identity and sequence. Copies and preview-control changes retain the source.
Content matching uses ContentId; it does not authorize use against a later
publication that happens to restore the same content. Observe checks both values.
Integration tests cover official CPU playback traces, seek replay and cache
limits, raw clip sampling, expression and pose composition, and curve sharing.
