# zipzi-project

Project persistence, asset management, and import and publication of model and animation resources.

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
