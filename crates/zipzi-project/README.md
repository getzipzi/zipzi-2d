# zipzi-project

Project persistence, asset management, and import and publication of model and animation resources.

The new `resource_store` module prepares immutable image versions independently
of any document/session. `ResourceView` retains those versions and checks the new
Core `DocumentState` image references before snapshot pairing. Files are captured
into owned, budgeted image content; retaining an external path is insufficient.
Views survive their originating store, and the last pixel/resource holder controls
reclamation. `ProjectIo` is the native SDK load/save entrypoint: load returns
DocumentState, ResourceView, project metadata and diagnostics; save accepts frozen
state/resources and returns an atomic-publication receipt. It owns no live model,
transaction or history. Resource keys are projected to archive paths without
rewriting model content. Missing images become explicit unresolved asset records
and retain their source description when saved again.

The existing DocumentSession-based import/export code described below remains a
stage-three migration input; it no longer owns formal SDK authoring state.
Focused checks: `cargo test --locked -p zipzi-project --lib archive::state_tests`
and `cargo test --locked -p zipzi-project --tests`.

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

`archive::ProjectArchiveStore` reads and atomically publishes native ZIP files:
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

The host supplies a private working-set parent and can share a
`ProjectDiskBudget` across stores. `ProjectResourceLease` retains immutable
resources until its last holder drops. Recognized abandoned working sets and
staging files can be reclaimed using owner metadata and locks. Staging is bounded,
streamed, finalized, synchronized and revalidated before publication. Replacement
requires a captured full-file fingerprint. A successful publish followed by a
directory-sync failure returns a published receipt with a durability warning.

The first resource allocation or open through a store reclaims abandoned working
sets on the I/O worker; Runtime store clones share that maintenance pass. Native
saves reclaim abandoned staging files in the selected destination directory before
creating a new stage. Cleanup skips live owners and unrecognized files; failures
are logged without changing publication or durability results.

Session readers admit ZIP indexes, expanded JSON/codec workspace and one PNG
decode against the shared host memory budget. Runtime retains the growing
reservation through completion; compressed archive length is not a decode-memory
estimate. `ProjectSaveProgress` reports preparation, actual uncompressed payload
bytes, finalization and publication independently of cancellation results.

Session and SDK entrypoints use this single-file backend. `project_path()` names
the archive; `asset_root()` names the private resource set. Save As retains that
set and history bindings. Snapshots, candidates and observation inputs retain
resource leases independently of their originating Session. The optional
`archive-prototype` example retains the explicitly marked v21 experiment solely
for reproducing the P0 measurements, which production readers reject.

```sh
cargo test --locked -p zipzi-project --lib archive::
cargo test --locked -p zipzi-project --test project_tests
cargo run --locked --release -p zipzi-project --features archive-prototype \
  --example project_archive_prototype -- fixtures target/archive-p0/fixtures
```

## Import artwork

`DocumentSession` imports of model3/MOC3 and CMO3/CAN3 normalize packed texture
pages into independent image assets for each Mesh. Runtime texture slots are
treated as atlas pages; editable Cubism imports identify active atlas inputs and
shared pages, keeping independent original artwork intact.

Extraction copies source texels in the Mesh's UV triangles, including the linear
sampler's edge footprint, and clears unrelated artwork inside the rectangular
crop. Integer cropping and UV rebasing preserve sampling for rotated or sheared
regions without baking or changing positions, vertex IDs, faces, masks or keyforms.
Missing/corrupt runtime pages retain their existing resource diagnostics.

Extracted images are bounded, in-memory drafts until saved or exported. Import
failure preserves the previous session. Runtime package and grouped transfer
outputs retain overwrite protection for the original input paths. Low-level
format decoders retain source UVs and pages for inspection.
Opening an existing Zipzi project does not repeat import normalization. Extraction
recovers the pixels available to the imported mesh, not missing original PSD art.

Session exports and Runtime exports of every format validate their destination
before encoding. They protect the original archive, tracked import sources and
disk images, including immutable working-set images retained only by history.
Path aliases are resolved through the same filesystem rules as publication.

Native projects use ordinary `.zipzi` ZIP files. Directory projects and bare JSON
are not user entrypoints. Bare JSON and experimental CBOR remain codec and
benchmark tools only; they do not add another production format.

Both native codecs retain optional Warp/Rotation original-shape snapshots,
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
