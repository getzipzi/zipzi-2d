# Test domains and CI

Run commands from the repository root. `tools/package-domains.json` assigns every
owned workspace package to exactly one domain: 15 SDK packages (including Python
and the Rust comparison runner) and 3 Editor packages. New workspace members must
be explicitly assigned; missing, stale, duplicate or overlapping names fail the
metadata coverage gate. Vendor packages are not workspace members.

```sh
python3 tools/cargo_domain.py --check
python3 tools/cargo_domain.py sdk check --locked --all-targets
python3 tools/cargo_domain.py editor check --locked --all-targets --features kasane-editor/test-support
./tools/test-sdk.sh fast physics_editing::
./tools/test-editor.sh ui i18n::
```

`cargo_domain.py` runs from the repository root, preserves Cargo status and
arguments, and prevents package selectors from overriding its domain. It is a
Python standard-library tool; `uv run --locked python ...` uses the project's
Python environment. `fmt` selects only the domain's own packages, excluding vendor.

| Entry | Coverage |
| --- | --- |
| SDK `fast` | kasane-sdk library and ordinary integration; optional filter |
| SDK `mesh` / `regression` | Daily alpha mesh / all 15 SDK libraries and integrations; skip only `alpha_mesh_stress::` |
| SDK `stress` | Entire fixed-seed alpha mesh matrix |
| SDK `python` / `wheel` | Existing Debug extension / isolated Release wheel gates |
| SDK `full` or no argument | SDK formatting, complete 15-package Cargo test invocation (examples/doctests included), then CPU wheel gate |
| Editor `fast` | editor-core library and synthetic integration |
| Editor `ui` / `regression` | All three Editor libraries / libraries plus synthetic integration, with test-support |
| Editor `acceptance` | acceptance, native_render, headless_screenshot, animation_acceptance, timeline_acceptance |
| Editor `tools` | Explicit Editor tooling tests plus shared domain controls |
| Editor `full` or no argument | Editor formatting, tooling and complete three-package Cargo test invocation with test-support (examples/doctests included) |

The SDK full gate's extra arguments retain their original wheel-gate meaning;
Editor full extra arguments go to Cargo tests. Focused Cargo arguments and arguments
after `--` retain their normal meaning. Editor Rust gates unset
`KASANE_DEV_SESSION_DIR` to isolate tests from live development sessions.
Formatting, Rust, tooling and wheel failures stop the relevant full gate and
propagate their exit status. Complete suites remain expensive: use focused modes
during iteration and reserve full runs for broad regression acceptance.

## CI coverage

Both workflows run on every push and pull request. There are no path filters:
SDK changes trigger Editor consumer validation; root manifests, shared tools,
assets and vendor changes cannot be missed by path heuristics.

SDK CI retains Linux/macOS/Windows, all-target clippy, decoder safety without
native Core, complete SDK tests, wheel building, isolated integration, Python
typing, validator negative controls, non-Windows experiment controls and the
macOS WGPU blend reference. Cargo commands select only SDK packages.

Editor CI runs on macOS: package coverage, formatting, tooling, every-target
compilation with test-support, and core synthetic tests. A Swift Metal-device
probe controls UI/integration and native acceptance execution. A failed probe
is a job failure. An absent device produces an explicit warning and job summary;
it is not evidence that UI/native tests passed. On a Metal-capable runner,
regression and all five acceptance targets must pass normally.

Licensed model data is not uploaded to CI. Optional missing-model cases retain
their existing skip behavior; `--nocapture` exposes their messages. Full model
acceptance stays in the local Editor acceptance/full gates after authorized data
is consolidated. Hosted runner execution and the final full regression/wheel
matrix remain tracked in the repository merge acceptance record.
