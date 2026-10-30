# zipzi-project

Project persistence, asset management, and import and publication of model and animation resources.

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

Native projects use `project.zipzi.json` with the `zipzi-directory-project`
format identifier and format version 21. Older names and experimental schemas
are unsupported; reimport the original source assets. The experimental CBOR
codec shares these wire rules and uses `project.zipzi.cbor`.

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
