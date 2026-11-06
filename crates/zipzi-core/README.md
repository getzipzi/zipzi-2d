# zipzi-core

Core data model for Zipzi 2D, including documents, geometry, deformers, parameter bindings, and scene evaluation.

The new `state` module completes the preparatory authoring kernel foundation.
`DocumentState` covers every persistent model domain, including scene/geometry,
bindings, organization, EditorState, animation, pose, physics and attachments.
`OwnedDocument` constructs a complete state without an old `Document` intermediary;
`DocumentBuilder` provides isolated typed writes and validated freezing. Shared
read-only model rules live in `model_validation`, without session or save state.
Directories, records, orders and metadata roots share independently. Geometry and
binding payloads currently share at record granularity; existing shared animation
curves and attachment buffers remain shared. Failed writes prevent freezing;
net-zero writes retain the original content identity. `WriteSet` records net
changes and a fixed conservative step estimate, deduplicating shared blocks only
within that step. Core stores resource keys only. Production consumers still use
`Document`; see the [implementation status](../../docs/plans/authoring-architecture.md).
Focused check: `cargo test --locked -p zipzi-core --lib state::`.

Rotation bone display baselines capture the evaluated inherited scale at parameter
defaults, including ordinary/BlendShape bindings and hidden ancestors. Existing
display preferences stay frozen through later shape edits and project reopening.

Document clones and checkpoints share Mesh, Transform, ordinary/BlendShape binding
records and per-mesh vertex indexes. Writes detach the affected record. Public
record types and project serialization remain owned values. Logical content/history
estimates include shared payloads; `estimated_snapshot_bytes` counts the metadata
and lookup descriptors copied by a candidate fork. `DocumentSnapshotId` supports
exact snapshot checks without retaining a document copy.
Candidate publication moves its maintained indexes along with content, preserving
shared indexes for untouched meshes. Checkpoint exchange rebuilds indexes for
the restored topology. Incremental validation reuses the geometry proof for
identical shared Mesh records while rechecking references in the candidate graph.

`image::decode_png_bounded` checks encoded bytes, expected header dimensions and
conservative decoder/conversion workspace before allocating output. Its decoder
bounds chunk storage, skips text/ICC decompression and validates checksums through
the image end. `png_decode_workspace_bytes` is the shared preparation upper bound;
`rgba_png_capacity` is the shared observation output capacity. Focused tests:
`cargo test --locked -p zipzi-core --lib image::`.
