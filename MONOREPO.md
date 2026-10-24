# Repository structure

The root Cargo workspace owns 18 crates: 15 SDK packages and 3 Editor packages.
`Cargo.toml` defines membership; [`tools/workspace/package-domains.json`](tools/workspace/package-domains.json)
assigns test domains. Validate both with `python3 tools/workspace/cargo_domain.py --check`
after adding or removing a member. All owned crates live directly under `crates/`.

## Crate responsibilities

| Crate | Responsibility |
| --- | --- |
| `kasane-core` | Editable documents, deformation and evaluation |
| `kasane-live2d` | Live2D motion, expression, pose and physics formats |
| `kasane-animation` | Animation evaluation |
| `kasane-project` | Project persistence, resources and package publication |
| `kasane-moc3` | MOC3 import/export and safety validation |
| `kasane-cmo3` | Cubism editor document import/export |
| `kasane-psd` | PSD import |
| `kasane-moc3-psd` | MOC3-to-PSD export |
| `kasane-render` | Backend-independent render planning |
| `kasane-render-wgpu` | WGPU rendering |
| `kasane-render-metal` | Native Metal rendering |
| `kasane-sdk` | Transactional Rust authoring API |
| `kasane-sdk-observe` | Immutable inspection and GPU capture |
| `kasane-python` | Python bindings and independently built wheel |
| `cubism-matrix-kasane` | SDK comparison benchmark runner |
| [`editor-core`](crates/editor-core/README.md) | UI-independent Editor state and SDK coordination |
| [`editor-render`](crates/editor-render/README.md) | Editor scene/overlay adapter to Metal |
| [`kasane-editor`](crates/kasane-editor/README.md) | GPUI application, panels and native integration |

The Editor consumes the SDK through workspace dependencies. `editor-core` has no
GPUI dependency; `editor-render` consumes its scene snapshots and the SDK's Metal
renderer; `kasane-editor` connects both to the UI. SDK crates must not depend on
Editor crates. The three Editor crates and their assets are GPL-3.0-only; SDK
crates are MIT. Shared directories do not imply a shared license:
[component licensing](LICENSING.md) remains authoritative.

## SDK and language boundaries

The Python wheel contains both PyO3 bindings and Python workflow code. Model
invariants and evaluation rules belong in the Rust core/format crates;
transactions, history and authoring rules belong in `kasane-sdk`; resource I/O
and publication belong in `kasane-project`. PyO3 converts values, errors and
results. Python provides user-facing types, context managers, batch workflows
and reports. New modeling rules belong in Rust even when Python is their only
current caller; model-specific experiments remain in experiment tooling.

`kasane-sdk-observe` captures immutable session snapshots and validates image
resources through `kasane-project`. Render backends share scene planning and CPU
geometry through `kasane-render`: native Metal on macOS and WGPU on other
platforms. See [format conversion boundaries](crates/kasane-cmo3/README.md) and
[coordinate contracts](docs/archive/coordinates.md) for format-specific details.

## Supporting directories

| Directory | Contents and ownership |
| --- | --- |
| `tools/` | Five daily entrypoints plus [functional tool groups](tools/README.md#directory-ownership) |
| `tools/tests/` | Tooling regression tests; see [test layout](tools/tests/README.md) |
| `fixtures/` | Shared test inputs and render references; see [fixture guide](fixtures/README.md) |
| `crates/<crate>/tests/` | Crate-owned integration/acceptance tests; Python extension tests stay with its crate |
| `crates/kasane-editor/assets/` | App icons, UI icons and locale resources |
| `docs/references/` | Topic references |
| `docs/plans/`, `docs/archive/` | Plans and historical design/measurement records; not current setup instructions |
| `examples/sdk/` | Public SDK usage examples |
| `models/` | Model inventory; private model files belong in ignored `local/` |
| `benchmarks/cubism-matrix/` | Independent official Cubism/Purism comparison project |
| `vendor/` | Pinned GPUI patches with their own licenses and excluded workspaces |
| `third_party/` | Local SDKs/assets plus the tracked `purism-core` submodule |
| `target/` | Disposable build artifacts, test environments, reports and app bundles |

## Shared infrastructure

The root `Cargo.lock` pins owned workspace builds. The default Cargo members are
`kasane-core`, `kasane-project` and `kasane-sdk`; plain `cargo test` is not a whole
repository gate. Use the SDK and Editor [test entrypoints](tools/README.md#test-coverage)
to keep platform requirements and package selection explicit. Vendor workspaces
and the Purism submodule have their own build/test lifecycles.

Root `pyproject.toml` and `uv.lock` pin development Python dependencies. Use
`uv add --group dev <package>` and commit both files when adding one. The Python
SDK retains its own build configuration under `crates/kasane-python`.

CI runs SDK and Editor consumer validation on every push and pull request.
Tooling tests are automatically discovered from `tools/tests/test_*.py`; native tests,
licensed fixtures and GPU checks retain explicit gates. See the
[tool guide](tools/README.md), [CI coverage](tools/README.md#test-coverage),
[dependency record](vendor/README.md#dependencies) and
[distribution guide](tools/README.md#local-distribution-artifacts).
