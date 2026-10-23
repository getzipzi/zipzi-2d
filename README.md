# Zipzi 2D

This checkout is integrating the Kasane SDK and Editor. SDK code is
MIT licensed; Editor code and assets retain GPL-3.0-only. See [licensing](LICENSING.md).
The workspace, tools and CI share explicit SDK (15 packages) and Editor
(3 packages) test domains; see [test gates](docs/development/testing.md).
See the [migration status](docs/repository-merge/README.md) and the
[revised flattening plan](docs/plans/repository-merge-plan.md). All 18 owned crates are flat under `crates/`; Editor assets, tools, docs and
third-party patches are integrated into root directories.
See the [Editor guide](docs/development/editor.md) and [documentation index](docs/README.md).

Kasane 2D provides a Rust authoring SDK, Python bindings, and a WGPU renderer

Start with the [Python SDK README](crates/kasane-python/README.md) and
[Python API reference](crates/kasane-python/API.md). See
[architecture](docs/ARCHITECTURE.md). The [model inventory](models/README.md) records
local models used by tests and benchmarks.

The [Runtime preparation record](docs/runtime/README.md) contains the R0 API
inventory, ownership audit and reproducible CPU fixture/baseline tools for the
paired Editor integration. Runtime and ConnectedBackend are not implemented yet.

## Build and test

The unified workspace requires Rust 1.95 or newer (Editor GPUI uses
`std::hint::cold_path`, stabilized in 1.95).

Python tools use uv (0.10.7 or newer) and CPython 3.14. The root
`pyproject.toml` and `uv.lock` manage development tools; the SDK wheel remains
an independently built package under `crates/kasane-python`.

```sh
uv sync --locked
cargo test --locked -p kasane-sdk
uv build --wheel --python 3.14 --out-dir target/python-wheels crates/kasane-python
uv run --locked python tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl
```

For repeated local edits, use the focused entrypoint:

```sh
./tools/test-sdk.sh fast                         # SDK unit + ordinary CPU integration
./tools/test-sdk.sh fast physics_editing::       # Filter one SDK domain
./tools/test-sdk.sh mesh                         # Daily alpha mesh regressions + boundary limits
./tools/test-sdk.sh regression                   # Rust libraries/integration, excluding stress remainder
./tools/test-sdk.sh stress                       # Complete alpha mesh suite + fixed-seed stress matrix
./tools/test-sdk.sh python                       # Current Debug extension + CPU/inspection tests
./tools/test-sdk.sh python --observe             # Current Debug extension + CPU/inspection/GPU
./tools/test-sdk.sh python -- -k history          # Filter Python tests
./tools/test-sdk.sh wheel                        # Real Release wheel + isolated acceptance
./tools/test-sdk.sh full                         # Format + complete Rust suite + CPU wheel gate
```

SDK ordinary integration cases share `--test integration`. Alpha mesh daily
regression keeps four representative cases from each original random matrix;
`stress` adds the remaining cases, preserving all 24 fragmented and 64 small
masks with the same seeds and assertions. `mesh` and `regression` skip only the
`alpha_mesh_stress::` tests. The 65536-vertex boundary case stays in daily checks.
Plain `cargo test --locked -p kasane-sdk` and `full` include the complete matrix;
stress tests are not ignored or feature-gated.
For a narrow Rust invocation use, for example,
`cargo test --locked -p kasane-sdk --test integration physics_editing::`.

The Python development gate keeps its environment in `target/python-tests/`
and reuses the workspace `target/` Cargo artifacts. Each invocation refreshes
the extension, checks its GPU capability, and starts fresh test processes.
This covers the current Rust and Python sources without rebuilding a Release
wheel on each edit. CPU/observe development runs share one editable environment
because Maturin installs their
native extension at the same source location. Packaged-wheel tests use separate
environments and import only the installed wheel. Complete wheel acceptance
remains a separate gate; its failures are propagated.

Run Python tools with `uv run --locked ...`. Add development dependencies with
`uv add --group dev <package>` and commit both `pyproject.toml` and `uv.lock`.
Agent experiments share one uv-managed environment per experiment and allow
participants to install dependencies; see the [experiment guide](docs/archive/SDK-EXPERIMENTS.md).

The wheel validator installs the wheel into a temporary environment outside
the source tree and tests project creation, import, editing, save, and export.
An observe-enabled wheel can also run GPU and pinned image-reference checks;
`--full` additionally requires official and Purism Core probes. See the
[tool guide](tools/README.md) for the complete commands.

## License and third-party rights

SDK components use [MIT](LICENSE-MIT); Editor components use
[GPL-3.0-only](LICENSE-GPL-3.0). Directory consolidation does not change
licensing. [LICENSING](LICENSING.md) identifies scopes and shared-file origins;
[third-party notices](THIRD_PARTY_NOTICES.md) retain dependency and benchmark exceptions.

Live2D, Cubism, the Live2D Cubism SDK, Cubism Core, Cubism Native Framework,
and associated sample data are owned by or licensed through Live2D Inc. and/or
their respective rightsholders. This project is independent and is not
affiliated with, authorized by, endorsed by, or sponsored by Live2D Inc. The
names “Live2D” and “Cubism” describe interoperability only.

This repository does not distribute the proprietary Cubism Core binary, a
Cubism SDK package, or Live2D sample model assets. Users who obtain, build,
link, publish, or distribute software using Live2D materials are responsible
for complying with all applicable terms, including the
[Live2D Proprietary Software License Agreement](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html),
[Live2D Open Software License Agreement](https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html),
and any applicable [sample data terms](https://www.live2d.com/eula/live2d-sample-model-terms_en.html).
The MIT License for this repository grants no rights to third-party software,
models, artwork, trademarks, or other materials.
