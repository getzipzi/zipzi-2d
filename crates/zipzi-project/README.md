# zipzi-project

Project persistence, asset management, and import and publication of model and animation resources.

The `resource_store` module prepares immutable image versions independently
of any document/session. `ResourceView` retains those versions and checks the new
Core `DocumentState` image references before snapshot pairing. Files are captured
into owned, budgeted image content; retaining an external path is insufficient.
Views survive their originating store. The final pixel or resource holder controls
reclamation. `ProjectIo` is the native SDK load/save entrypoint: load returns
DocumentState, ResourceView, project metadata and diagnostics; save accepts frozen
state/resources and returns an atomic-publication receipt. It owns no live model,
transaction or history. Resource keys are projected to archive paths without
rewriting model content. Missing images become explicit unresolved asset records
and retain their source description when saved again.

`ProjectImporter` returns `ImportedProject` values containing frozen state,
owned resources and protected input paths. Model3/MOC3, CMO3/CAN3, PSD and
sidecar adapters use Builder and frozen projections. Project no longer contains
DocumentSession or DocumentStore; the caller decides when to publish an import.

Focused checks:

```sh
cargo test --locked -p zipzi-project --lib
cargo test --locked -p zipzi-project --test authoring_formats
cargo test --locked -p zipzi-project --test cdi_tests --test expression_tests --test motion_tests --test pose_tests --test physics_tests
cargo test --locked -p zipzi-project --test cmo3_tests --test cubism_tests
```

Sidecar tests exercise native ZIP save/reopen, stable identities, reference
validation, opaque metadata and failed package imports. Cubism tests cover
captured resources, native-project equivalence and grouped output replacement.
The optional `--test mao_cubism -- --nocapture` gate uses the local licensed Mao
sample; it reports a skip when the model is absent.

The legacy `project_tests` and `sequence_tests` targets remain stage-five migration
inputs; the focused gates above do not establish a full Project regression pass.

## Filesystem guarantees

Saves synchronize new file contents before publication. Unix additionally attempts
directory synchronization. Windows publication reports a durability warning because
directory synchronization is unavailable; successful publication does not promise
that the directory entry survives power loss. A destination opened without delete
sharing blocks replacement; the existing destination survives and callers may retry
after releasing the handle. Project locks use a persistent lock file, with flock
on Unix and an exclusive file handle on Windows. Never delete the lock file to
resolve a busy project.

Focused native controls: `cargo test --locked -p zipzi-project --lib filesystem::tests
-- --nocapture`. Windows symlink tests require Developer Mode or the relevant
privilege and report missing coverage explicitly when unavailable. Ordinary file,
case-alias, lock and replacement tests always execute.

## Native project container

`ProjectIo` reads and atomically publishes native ZIP files:
container version 1, document identifier `zipzi-project`, document version 22.
An optional `preview.png` and `format.json.preview` descriptor carry a static
RGBA8 image (maximum 1024 × 1024) and SHA-256. This does not change the container
version or add a required feature. The image is advisory; the Editor ignores its
pixels and malformed preview metadata when opening authoring content. Shell
readers validate it independently through `zipzi-project-preview`.
Stored PNG entries use complete SHA-256 names; JSON uses Deflate. ZIP admission
checks central-directory limits before indexing, validates ZIP64, local headers,
data descriptors and non-overlapping regions. Unsupported entries and required
features fail explicitly. Missing or corrupt images produce repair diagnostics;
they cannot silently become missing content during save.

The host supplies a private working directory and can share disk and image
memory budgets. ResourceView keeps captured immutable image versions alive
independently of the importer or Session. Staging is bounded, streamed, finalized,
synchronized and revalidated before publication. Replacement requires a captured
full-file fingerprint. Publication followed by directory-sync failure returns a
published receipt with a durability warning.

I/O reclaims abandoned working sets and destination staging files using owner
metadata and locks. Cleanup skips live owners and unrecognized files; failures
are logged without changing publication results. Readers admit ZIP indexes,
expanded JSON/codec workspace and PNG decode against the supplied memory budget.
ProjectSaveProgress reports preparation, actual uncompressed payload bytes,
finalization and publication independently of cancellation results.

Native load and save project resource keys into archive entries at the file boundary.

## Import artwork

`ProjectImporter` imports of model3/MOC3 and CMO3/CAN3 normalize packed texture
pages into independent image assets for each Mesh. Runtime texture slots are
treated as atlas pages; editable Cubism imports identify active atlas inputs and
shared pages, keeping independent original artwork intact.

Extraction copies source texels in the Mesh's UV triangles, including the linear
sampler's edge footprint, and clears unrelated artwork inside the rectangular
crop. Integer cropping and UV rebasing preserve sampling for rotated or sheared
regions without baking or changing positions, vertex IDs, faces, masks or keyforms.
Missing/corrupt runtime pages retain their existing resource diagnostics.

Extracted images are bounded, owned resource values until saved or exported.
Import performs no Session mutation. Runtime package and grouped transfer
outputs retain overwrite protection for the original input paths. Low-level
format decoders retain source UVs and pages for inspection.
Opening an existing Zipzi project does not repeat import normalization. Extraction
recovers the pixels available to the imported mesh, not missing original PSD art.

TransferSnapshot and export plans capture frozen state and ResourceView.
Transfer publication protects supplied project/import source paths; path aliases
use the same filesystem rules as publication. SDK/Runtime task wiring belongs to
the host migration stage.

Native projects use ordinary `.zipzi` ZIP files. Directory projects and bare JSON
are not user entrypoints. JSON encoding is private to the native archive boundary;
the experimental directory and CBOR codecs have been removed.

The native codec retains optional Warp/Rotation original-shape snapshots,
their canvas coordinates, capture source, automatic-update policy and first-key
marker. Missing records remain missing when older projects are opened. Copies
and undo retain these records; exchange formats such as CMO3 report their loss.

Motion initial keys and segment endpoints carry persistent UUIDs; Bezier
controls remain anonymous points. Key IDs are excluded from motion3 arrays and
opaque-content fingerprints; reimporting a motion allocates fresh key identities.

## Bounded observation resources

`read_project_texture_bounded` checks file length, fixed PNG header dimensions,
encoded/workspace limits and declared SHA256 before returning verified pixels.
Reads remain bounded if the file grows; truncation, corruption and content changes
fail explicitly. Cancellation is checked between read/hash chunks and after decode.
`PreparedImageContent::verified_texture` validates memory metadata and shares the
existing immutable pixel Arc without populating its PNG cache. Memory identities
use domain-tagged straight RGBA8 hashes with dimensions; file identities use the
verified PNG hash. Estimates do not verify external resource contents.

Focused checks: `cargo test --locked -p zipzi-project --lib texture::tests` and
`cargo test --locked -p zipzi-project --lib resource_budget`.
