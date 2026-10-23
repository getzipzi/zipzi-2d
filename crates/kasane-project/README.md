# kasane-project

Project persistence, asset management, and import and publication of model and animation resources.

The JSON writer emits directory-project format v11. Motion initial keys and
segment endpoints carry persistent UUIDs; Bezier controls remain anonymous
points. Motion records from earlier versions are not migrated. Existing
non-motion schemas in v1–v10 remain readable. The experimental CBOR codec shares
these wire rules. Key IDs are excluded from motion3 arrays and opaque-content
fingerprints; reimporting a motion allocates fresh key identities.
