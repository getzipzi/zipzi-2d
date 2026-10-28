# Repository structure

The root Cargo workspace owns 20 crates: 17 SDK packages and 3 Editor packages.
`Cargo.toml` defines membership; [`tools/workspace/package-domains.json`](tools/workspace/package-domains.json)
assigns test domains. Validate both with `python3 tools/workspace/cargo_domain.py --check`
after adding or removing a member. All owned crates live directly under `crates/`.

## Crate responsibilities

| Crate | Responsibility |
| --- | --- |
| `zipzi-core` | Editable documents, deformation and evaluation |
| `zipzi-live2d` | Live2D motion, expression, pose and physics formats |
| `zipzi-animation` | Animation evaluation |
| `zipzi-project` | Project persistence, resources and package publication |
| `zipzi-moc3` | MOC3 import/export and safety validation |
| `zipzi-cmo3` | Cubism editor document import/export |
| `zipzi-psd` | PSD import |
| `zipzi-moc3-psd` | MOC3-to-PSD export |
| `zipzi-render` | Backend-independent render planning |
| `zipzi-render-wgpu` | WGPU rendering |
| `zipzi-render-metal` | Native Metal rendering |
| `zipzi-sdk` | Transactional Rust authoring API |
| [`zipzi-runtime`](crates/zipzi-runtime/README.md) | UI-independent hosted sessions, operations, jobs and events |
| `zipzi-observe` | Immutable inspection and GPU capture |
| [`zipzi-python2`](crates/zipzi-python2/README.md) | Independent typed Python client and optional Runtime-native adapter |
| `cubism-matrix-zipzi` | SDK comparison benchmark runner |
| [`zipzi-editor-core`](crates/zipzi-editor-core/README.md) | UI-independent Editor state and SDK coordination |
| [`zipzi-editor-render`](crates/zipzi-editor-render/README.md) | Editor scene/overlay adapter to Metal |
| [`zipzi-editor`](crates/zipzi-editor/README.md) | GPUI application, panels and native integration |

The Editor consumes the SDK through workspace dependencies. `zipzi-editor-core` has no
GPUI dependency; `zipzi-editor-render` consumes its scene snapshots and the SDK's Metal
renderer; `zipzi-editor` connects both to the UI. SDK crates must not depend on
Editor crates. The three Editor crates and their assets are GPL-3.0-only; SDK
crates are MIT. Shared directories do not imply a shared license:
[component licensing](LICENSING.md) remains authoritative.

## SDK and language boundaries

`zipzi-python2` builds a pure Python client and an optional native adapter as
separate distributions. Both backends enter the shared Runtime. Model
invariants and evaluation rules belong in the Rust core/format crates;
transactions, history and authoring rules belong in `zipzi-sdk`; resource I/O
and publication belong in `zipzi-project`. PyO3 converts values, errors and
results. Python provides user-facing types, context managers, batch workflows
and reports. New modeling rules belong in Rust even when Python is their only
current caller; model-specific experiments remain in experiment tooling.

`zipzi-observe` captures immutable session snapshots and validates image
resources through `zipzi-project`. Render backends share scene planning and CPU
geometry through `zipzi-render`: native Metal on macOS and WGPU on other
platforms. See [format conversion boundaries](crates/zipzi-cmo3/README.md) and
[coordinate contracts](docs/archive/coordinates.md) for format-specific details.

## Supporting directories

| Directory | Contents and ownership |
| --- | --- |
| `tools/` | Five daily entrypoints plus [functional tool groups](tools/README.md#directory-ownership) |
| `tools/tests/` | Tooling regression tests; see [test layout](tools/tests/README.md) |
| `fixtures/` | Shared test inputs and render references; see [fixture guide](fixtures/README.md) |
| `crates/<crate>/tests/` | Crate-owned integration/acceptance tests; Python package tests stay with their crate |
| `crates/zipzi-editor/assets/` | App icons, UI icons and locale resources |
| `docs/references/` | Topic references |
| `docs/plans/`, `docs/archive/` | Plans and historical design/measurement records; not current setup instructions |
| `models/` | Model inventory; private model files belong in ignored `local/` |
| `benchmarks/cubism-matrix/` | Independent official Cubism/Purism comparison project |
| `vendor/` | Pinned GPUI patches with their own licenses and excluded workspaces |
| `third_party/` | Local SDKs/assets plus the tracked `purism-core` submodule |
| `target/` | Disposable build artifacts, test environments, reports and app bundles |

## Shared infrastructure

The root `Cargo.lock` pins owned workspace builds. The default Cargo members are
`zipzi-core`, `zipzi-project` and `zipzi-sdk`; plain `cargo test` is not a whole
repository gate. Use the SDK and Editor [test entrypoints](tools/README.md#test-coverage)
to keep platform requirements and package selection explicit. Vendor workspaces
and the Purism submodule have their own build/test lifecycles.

Root `pyproject.toml` and `uv.lock` pin development Python dependencies. Use
`uv add --group dev <package>` and commit both files when adding one. The Python
Runtime-backed Python SDK has its native build configuration under
`crates/zipzi-python2` and pure client configuration under its `client/` directory.

CI runs SDK and Editor consumer validation on every push and pull request.
Tooling tests are automatically discovered from `tools/tests/test_*.py`; native tests,
licensed fixtures and GPU checks retain explicit gates. See the
[tool guide](tools/README.md), [CI coverage](tools/README.md#test-coverage),
[dependency record](vendor/README.md#dependencies) and
[distribution guide](tools/README.md#local-distribution-artifacts).
