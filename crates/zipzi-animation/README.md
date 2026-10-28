# zipzi-animation

Deterministic CPU evaluation of motions, expressions, poses and physics.

`MotionPreview` evaluates playback into independently owned frames. It reuses
per-preview scratch storage; concurrent reads of the same preview serialize.
`MotionEditingPreview` samples a raw clip from the default pose without motion
fades, expressions, physics or events, and resolves Pose immediately.
`motion_positions` and `canonical_motion_seek_time` expose playback coordinates
for timeline seeking.

Motion tracks and events use copy-on-write storage. Edits validate the affected
fields and dependencies; SDK publication validates the complete document.

```sh
cargo test --locked -p zipzi-animation
```
