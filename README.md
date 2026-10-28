# Zipzi 2D

Zipzi 2D brings the Zipzi Rust authoring SDK, Python bindings and native macOS
Editor into one workspace. It supports editable 2D model documents, animation,
Live2D/Cubism format interoperability, and WGPU/Metal rendering. The project is
under development with no stable release; APIs and file formats may change.
See [release preparation and Alpha downloads](docs/RELEASING.md).

## Start here

- **Desktop Editor:** [setup, development and native tests](crates/zipzi-editor/README.md).
- **Rust SDK:** [authoring API](crates/zipzi-sdk/README.md) and
  [crate responsibilities](MONOREPO.md#crate-responsibilities).
- **Python SDK:** [independent Runtime client](crates/zipzi-python2/README.md),
  [implemented capabilities](crates/zipzi-python2/CAPABILITIES.md) and
  [Runtime R0–R4 integration](docs/archive/plans/sdk-runtime/python2-r0-r4.md) and
  [R5/P5 animation authoring](docs/archive/plans/sdk-runtime/python2-r5-authoring.md).
  [R5/R7 final acceptance](docs/archive/plans/sdk-runtime/python2-r7-closeout.md) records
  the current macOS/CPython 3.14 distribution, complete workflows and performance.
  [Python P7 closeout](docs/archive/plans/python-sdk-rewrite/p7.md) reconciles the complete
  Python contract, capability evidence, ownership boundaries and measured costs.
- **Contributing:** [repository structure](MONOREPO.md),
  [test gates](tools/README.md#test-coverage), [tools](tools/README.md) and
  [documentation index](docs/README.md).

## Development

Use Rust 1.95 or newer, uv 0.10.7 or newer, and CPython 3.14.
The Editor additionally requires macOS, Xcode Command Line Tools and a
Metal-capable GPU. The [Windows port](crates/zipzi-editor/README.md) will use the
shared wgpu backend; Windows rendering integration is pending.
Run these commands from the repository root:

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

For the new Python SDK, `./tools/test-sdk.sh python2` refreshes its native adapter
and runs Local/Connected behavior and static types. `python2-connected` installs
the pure wheel and validates real subprocess host discovery and session binding;
add `--editor-host /absolute/path/to/zipzi-editor` for the current desktop host.
Root `pyproject.toml` and `uv.lock` manage development tools.

## Tests

Choose the smallest gate covering the change, then broaden when needed. Run
commands from the repository root; inspect available modes with
`./tools/test-sdk.sh --help`, `./tools/test-editor.sh --help` or
`uv run --locked python tools/test_tools.py --help`.

| Changed area | Starting command |
| --- | --- |
| One Rust crate/module | `cargo test --locked -p <crate> --lib <filter>`; add its integration target when relevant |
| Runtime ownership/jobs/events | `./tools/test-sdk.sh runtime` |
| Local protocol / installed Python client | `./tools/test-sdk.sh python2-connected` |
| SDK authoring behavior | `./tools/test-sdk.sh fast <filter>` |
| Editor adaptive mesh generation | `./tools/test-editor.sh fast adaptive_mesh::`; `./tools/test-editor.sh fast mesh_generation::` |
| SDK alpha mesh generation | `./tools/test-sdk.sh mesh` |
| wgpu backend / local GPU parity | `cargo test --locked -p zipzi-render-wgpu --tests`; [host selection](crates/zipzi-render-wgpu/README.md#application-routes-on-macos) |
| Editor coordination | `./tools/test-editor.sh fast <filter>` |
| Editor UI (macOS/Metal) | `./tools/test-editor.sh ui <filter>` |
| Runtime Python SDK (`zipzi-python2`), Local/Connected and types | `./tools/test-sdk.sh python2` ([scope](crates/zipzi-python2/README.md)) |
| Runtime GPU observation / inspection (usable GPU required) | `./tools/test-sdk.sh python2 --observation`; [scope and evidence](docs/archive/plans/sdk-runtime/python2-r5-observation.md) |
| Python complete-call recovery, Edit state and Job timeout | `./tools/test-sdk.sh python2 --recovery` (optional `P3ARecovery.test_name`) |
| Python collection pages, Session detach/close and offline catalog | `./tools/test-sdk.sh python2 --discovery` ([P3A-6 results and remaining gates](docs/archive/plans/python-sdk-rewrite/p3a-6.md)) |
| Installed new client, host discovery and session binding | `./tools/test-sdk.sh python2-connected` (optional `--editor-host /absolute/path/to/zipzi-editor`) |
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
