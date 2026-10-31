# Working in this repository

This project has not been officially released. Breaking refactors are allowed;
migrations do not need to be considered.

Use this file as the development entrypoint. Reading the root README is not
required; it is the product introduction. Consult the affected crate's README
and local instructions when you need implementation details or focused tests.

## Development setup

Run commands from the repository root. Required tools:

- Rust 1.95 or newer, uv 0.10.7 or newer, and CPython 3.14.
- macOS Editor: Xcode Command Line Tools and a Metal-capable GPU.
- Windows Editor development prototype: MSVC C++ Build Tools, Windows SDK and
  a usable GPU. Desktop validation is still in progress.

```sh
uv sync --locked
uv run --locked python tools/workspace/cargo_domain.py --check
```

All owned crates share the root Cargo workspace and lockfile. Package domains
are defined in `tools/workspace/package-domains.json`; update that inventory
when adding or moving workspace members. Root `pyproject.toml` and `uv.lock`
manage Python development tools. Use `uv run --locked python` for repository
Python tooling.

## Run the Editor and SDK scripts

```sh
# Cross-platform development launcher
uv run --locked python tools/dev.py editor run
uv run --locked python tools/dev.py editor reload
uv run --locked python tools/dev.py editor stop

# Rebuild on source changes
uv run --locked python tools/dev.py editor watch

# Direct native Editor launch
cargo run --locked -p zipzi-editor
```

For ad hoc SDK scripts, use `./tools/dev-python`, not global Python or a custom
`PYTHONPATH`. It prepares CPython 3.14, installs the editable client and
incrementally builds the native adapter before execution. Keep disposable
scripts and output under `target/python2/scratch/` or a temporary directory.

```sh
./tools/dev-python target/python2/scratch/probe.py

# Cross-platform equivalent, including Windows
uv run --locked python tools/dev.py python target/python2/scratch/probe.py
```

See the [internal SDK scripting reference](docs/references/dev-python.md) for
lifecycle and environment details. Connected scripts use the selected running
host; rebuild or restart the Editor separately when its code changes.

## Choose focused tests

Start with the smallest package/module gate covering the change. Full SDK and
Editor suites are expensive; reserve them for broad regression coverage.
Filters, crate names and filenames below are placeholders; replace them with
the affected target. Omit a filter only when the entire selected gate is needed.

| Changed area | Starting command |
| --- | --- |
| One Rust crate/module | `cargo test --locked -p <crate> --lib <filter>` |
| A crate's integration behavior | `cargo test --locked -p <crate> --test <target> <filter>` |
| SDK authoring behavior | `./tools/test-sdk.sh fast <filter>` |
| Runtime sessions, ownership, jobs, events or protocol | `./tools/test-sdk.sh runtime <filter>` |
| SDK alpha mesh generation | `./tools/test-sdk.sh mesh` |
| Editor core coordination | `./tools/test-editor.sh fast <filter>` |
| Editor adaptive mesh generation | `./tools/test-editor.sh fast adaptive_mesh::`; `./tools/test-editor.sh fast mesh_generation::` |
| Editor UI logic | `./tools/test-editor.sh ui <filter>` |
| wgpu backend / GPU parity | `cargo test --locked -p zipzi-render-wgpu --tests` (requires a usable GPU) |
| Python SDK Local/Connected behavior, types and generation drift | `./tools/test-sdk.sh python2` |
| Installed Python client, host discovery and session binding | `./tools/test-sdk.sh python2-connected` |
| Production Editor host with the installed Python client | `./tools/test-sdk.sh python2-connected --editor-host /absolute/path/to/zipzi-editor` |
| Python GPU observation / inspection | `./tools/test-sdk.sh python2 --observation` (requires a usable GPU) |
| Python call recovery, Edit state or Job timeout | `./tools/test-sdk.sh python2 --recovery` |
| Python pagination, Session detach/close or offline catalog | `./tools/test-sdk.sh python2 --discovery` |
| Shared authoring contract / isolated Python generation | `cargo test --locked -p zipzi-contract`; `uv run --locked python tools/test_tools.py -p test_python2_codegen.py` |
| Repository tooling | `uv run --locked python tools/test_tools.py -p test_<tool>.py` |

For integration targets, inspect the affected crate's README and `tests/`, or
list cases with `cargo test --locked -p <crate> --test <target> -- --list`.
Plain `cargo test` selects only `zipzi-core`, `zipzi-project` and `zipzi-sdk`;
it is not a full workspace test.

Editor shell gates clear `ZIPZI_DEV_SESSION_DIR`. Clear it explicitly for direct
Editor Cargo commands so tests do not attach to a live development session:

```sh
# macOS / POSIX shell
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --lib <filter>
```

In PowerShell, use `Remove-Item Env:ZIPZI_DEV_SESSION_DIR -ErrorAction SilentlyContinue`
before direct Editor Cargo commands. Windows supports UI logic tests;
Metal native acceptance and screenshot tests require macOS and Metal.

## Test entrypoints and broader coverage

The cross-platform dispatcher is `tools/dev.py`; shell scripts are wrappers:

```sh
uv run --locked python tools/dev.py test sdk fast <filter>
uv run --locked python tools/dev.py test editor ui <filter>
uv run --locked python tools/dev.py test tools -p test_<tool>.py

./tools/test-sdk.sh --help
./tools/test-editor.sh --help
uv run --locked python tools/test_tools.py --help
```

- SDK modes: `fast`, `runtime`, `mesh`, `regression`, `stress`, `python2`,
  `python2-connected`, `full`.
- Editor modes: `fast`, `ui`, `regression`, `acceptance`, `tools`, `full`.
- No mode on either domain shell script displays help. Select `full` explicitly
  when broad coverage is warranted.
- Cargo filters and arguments after `--` retain their normal meaning.
- Tooling tests live in `tools/tests/`; crate tests stay with their crate;
  shared test data lives in `fixtures/`. Local model assets belong in
  `models/local/` and may be required for opt-in acceptance tests.

[Tooling coverage and prerequisites](tools/README.md#test-coverage) define the
broader regression, stress, native acceptance and packaged-wheel gates.
See [CI coverage](tools/README.md#ci-coverage) for CI expectations and
[distribution checks](tools/README.md#local-distribution-artifacts) for source
archives, Python wheels and application bundles. Use [MONOREPO.md](MONOREPO.md)
for crate responsibilities and [docs/README.md](docs/README.md) to find topic
references as needed.
