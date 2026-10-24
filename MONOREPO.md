# Runtime monorepo

## Active authoring and rendering

- `crates/kasane-core/` — editable document, deformation and evaluation.
- `crates/kasane-project/` — project persistence, resource validation and package publication.
- `crates/kasane-moc3/` — MOC3 import, export and safety checks.
- `crates/kasane-render/` — renderer-independent scene plan.
- `crates/kasane-render-wgpu/` — WGPU renderer.
- `crates/kasane-render-metal/` — native Metal renderer sharing CPU planning with WGPU.
- `crates/kasane-sdk/` — transactional Rust authoring API.
- `crates/kasane-sdk-observe/` — immutable observation and GPU capture.
- `crates/kasane-python/` — Python wheel and agent scripting interface.
- `tests/` — acceptance-gate tests and shared model, texture and image-reference fixtures.
- `models/` — local Live2D model sources and their usage inventory; small generated test fixtures stay in `tests/fixtures/`.
- `tools/` — SDK, Core and WGPU acceptance commands.

## Separate comparison projects

- `third_party/purism-core/` — Cubism Core-compatible implementation.
- `benchmarks/cubism-matrix/` — independent official Cubism/Purism comparison benchmark.
- `third_party/` — local, untracked SDKs and assets.

## Editor and shared infrastructure

- `crates/kasane-editor/`, `crates/editor-core/`, `crates/editor-render/` — GPL Editor UI, authoring and rendering.
- `crates/cubism-matrix-kasane/` — SDK Rust comparison runner.
- `crates/kasane-editor/assets/{app-icons,icons,locales}/` — application and packaging assets owned by the Editor crate.
- `vendor/` — third-party GPUI patches; inner workspaces remain independent.
- `tools/dev-editor.sh`, `tools/build-editor-app.sh`, `tools/test-editor.sh` — Editor entrypoints.
- `tools/test-sdk.sh` — SDK entrypoint. Both test entrypoints and CI use `tools/package-domains.json`; new members must be assigned to one domain.
- `docs/` — combined plans, references, development guides and historical records.
- `tools/package_source.py`, `tools/verify_artifacts.py` — workspace source and built-artifact validation; see [distribution](docs/development/distribution.md).

All 18 owned Cargo members live directly in `crates/<package.name>`. See
[licensing](LICENSING.md) before reusing shared tools or docs. `third_party/purism-core`
is the tracked submodule exception within otherwise ignored local third-party material.
