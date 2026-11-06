# zipzi-core

Core data model for Zipzi 2D, including documents, geometry, deformers, parameter bindings, and scene evaluation.

`DocumentState` is the immutable model. `OwnedDocument` accepts owned decoding
inputs; `DocumentBuilder` performs isolated typed writes, normalizes identities,
validates the complete graph and freezes a new state. Core owns no Session,
transaction, saved baseline, checkpoint or publication state. The old mutable
`Document` and its plan executors have been removed.

Directories, records, orders and metadata roots share independently. Geometry and
binding payloads share at record granularity; animation curves and attachment
buffers retain their existing sharing. Failed writes prevent freezing; net-zero
writes retain ContentId. Natural touched-record tracking produces WriteSet and
its fixed conservative history-step estimate. That estimate is not a conversion
admission limit. Images store ResourceKey and dimensions, or an explicit empty or
unresolved value; Core does not own image files.

Builder commands cover parameters, ordinary/BlendShape bindings, sampling and
range/grid conversions, scene and organization edits, mesh topology, reparenting,
grouping, dependency deletion, Motion, Expression, Pose, Physics, references,
attachments and EditorState. Typed field patches retain expected values without
address registries. Read-only domain algorithms live in `model_validation`;
Builder installs their affected records through the same tracking. Domain locks,
identity remapping and orphan metadata cleanup apply to these commands.
Conversion limits bound sample combinations and actual temporary/output payloads,
independently of retained history.

Evaluation, reference/export projections and preview caches consume frozen state.
Rotation bone display baselines capture the default evaluated inherited scale,
including ordinary/BlendShape bindings and hidden ancestors. Imported numeric
axes are normalized before initialization. Existing display preferences remain
stable through later edits and reopening.

The unit and integration tests exercise Builder validation, snapshot isolation,
parameter and keyform evaluation, Glue and Offscreen behavior, and warmed
evaluation storage.

```sh
cargo test --locked -p zipzi-core
cargo check --locked -p zipzi-core --all-targets
```

`image::decode_png_bounded` checks encoded bytes, expected header dimensions and
conservative decoder/conversion workspace before allocating output. Its decoder
bounds chunk storage, skips text/ICC decompression and validates checksums through
the image end. `png_decode_workspace_bytes` is the shared preparation upper bound;
`rgba_png_capacity` is the shared observation output capacity. Focused tests:
`cargo test --locked -p zipzi-core --lib image::`.
