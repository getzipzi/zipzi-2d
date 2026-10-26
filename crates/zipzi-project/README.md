# zipzi-project

Project persistence, asset management, and import and publication of model and animation resources.

Native projects use `project.zipzi.json` with the `zipzi-directory-project`
format identifier and format version 21. Older names and experimental schemas
are unsupported; reimport the original source assets. The experimental CBOR
codec shares these wire rules and uses `project.zipzi.cbor`.

Motion initial keys and segment endpoints carry persistent UUIDs; Bezier
controls remain anonymous points. Key IDs are excluded from motion3 arrays and
opaque-content fingerprints; reimporting a motion allocates fresh key identities.
