# Shared fixtures

Small model, animation, texture and image-reference data shared by Rust crates,
the Python SDK and validation tools. Crate-private data stays in each crate's
`tests/fixtures/`; tooling regression tests live in `tools/tests/`.

| Directory | Contents |
| --- | --- |
| `animation_cpu/` | Synthetic motion, expression, pose and physics inputs |
| `external_v42/`, `external_v50/`, `external_v50_bs_glue/`, `external_cyclic/` | MOC3 interoperability fixtures and associated resources |
| `gpu/` | Shared GPU package inputs |
| `observe_inspection/` | Small authored projects and textures for inspection |
| `render_reference/` | Pinned render captures and provenance metadata |

Fixture generators live in `tools/fixtures/create_*` and
`observe_inspection/build_fixtures.py`. Run them from the repository root using
the documented SDK environment; rebuilding references is an explicit operation.

Keep fixtures small and reproducible. Preserve provenance, hashes and capture
settings when changing render references. Do not copy licensed local models into
tracked fixtures; those belong in ignored `models/local/`. See
[model inventory](../models/README.md) and [third-party notices](../THIRD_PARTY_NOTICES.md).
