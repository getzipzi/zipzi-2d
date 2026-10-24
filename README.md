# Zipzi 2D

Zipzi 2D brings the Zipzi Rust authoring SDK, Python bindings and native macOS
Editor into one workspace. It supports editable 2D model documents, animation,
Live2D/Cubism format interoperability, and WGPU/Metal rendering. The project is
under development and has not been officially released; APIs and file formats
may change.

## Start here

- **Desktop Editor:** [setup, development and native tests](crates/zipzi-editor/README.md).
- **Rust SDK:** [authoring API](crates/zipzi-sdk/README.md) and
  [crate responsibilities](MONOREPO.md#crate-responsibilities).
- **Python SDK:** [installation and examples](crates/zipzi-python/README.md),
  [API reference](crates/zipzi-python/API.md), [recipes](examples/sdk/README.md).
- **Contributing:** [repository structure](MONOREPO.md),
  [test gates](tools/README.md#test-coverage), [tools](tools/README.md) and
  [documentation index](docs/README.md).

## Development

Use Rust 1.95 or newer, uv 0.10.7 or newer, and CPython 3.14.
The Editor additionally requires macOS, Xcode Command Line Tools and a
Metal-capable GPU. Run these commands from the repository root:

```sh
uv sync --locked
python3 tools/workspace/cargo_domain.py --check

# Focused SDK development
cargo test --locked -p zipzi-sdk --test integration physics_editing::

# Native Editor (macOS)
cargo run --locked -p zipzi-editor
./tools/test-editor.sh fast timeline::

# Repository tooling tests; no native application build
uv run --locked python tools/test_tools.py
```

For Python development, `./tools/test-sdk.sh python` refreshes the Debug extension
before running CPU and inspection tests; add `--observe` for GPU coverage.
`./tools/test-sdk.sh wheel` builds a Release wheel and validates its isolated
installation. The wheel is an independent package under `crates/zipzi-python`;
root `pyproject.toml` and `uv.lock` manage development tools.

## Tests

Choose the smallest gate covering the change, then broaden when needed. Run
commands from the repository root; inspect available modes with
`./tools/test-sdk.sh --help`, `./tools/test-editor.sh --help` or
`uv run --locked python tools/test_tools.py --help`.

| Changed area | Starting command |
| --- | --- |
| One Rust crate/module | `cargo test --locked -p <crate> --lib <filter>`; add its integration target when relevant |
| Runtime ownership/jobs/events | `./tools/test-sdk.sh runtime` |
| SDK authoring behavior | `./tools/test-sdk.sh fast <filter>` |
| Alpha mesh generation | `./tools/test-sdk.sh mesh` |
| Editor coordination | `./tools/test-editor.sh fast <filter>` |
| Editor UI (macOS/Metal) | `./tools/test-editor.sh ui <filter>` |
| Python SDK | `./tools/test-sdk.sh python` (add `--observe` for GPU coverage) |
| Repository tools | `uv run --locked python tools/test_tools.py -p test_<tool>.py` |

Filters and filenames are placeholders; omit them to run the selected gate.
For a crate's integration targets, inspect its README and `tests/`, or list cases
with `cargo test --locked -p <crate> --test <target> -- --list`.
Editor shell gates clear `ZIPZI_DEV_SESSION_DIR`; clear it explicitly when
running Editor Cargo tests directly.

[Full coverage, prerequisites and CI](tools/README.md#test-coverage) define
regression, stress, native acceptance and packaged-wheel gates. Both domain
scripts default to expensive **full** runs when no mode is supplied.
Plain `cargo test` selects only `zipzi-core`, `zipzi-project` and `zipzi-sdk`.
Tooling tests belong in `tools/tests/`, crate tests stay with their crate, and
shared data belongs in [`fixtures/`](fixtures/README.md).

All owned crates share the root Cargo workspace and lockfile. Local models belong
in `models/local/`; see the [model inventory](models/README.md). Source archives,
Python wheels and macOS app bundles have separate
[distribution checks](tools/README.md#local-distribution-artifacts).

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
