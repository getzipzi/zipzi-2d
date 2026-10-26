# Zipzi Python SDK (deprecated)

**Deprecated: this package is no longer maintained.** It is retained only for
historical tests and experiments; new features and compatibility fixes belong in
[`zipzi-python2`](../zipzi-python2/README.md). Its Connected client is incompatible
with the current Runtime protocol.

The maintained `zipzi` / `zipzi-native` release sources are in
[`../zipzi-python2`](../zipzi-python2/README.md). Do not upload this legacy wheel
to the public `zipzi` project; see the [release guide](../../docs/RELEASING.md).

The remaining documentation describes the deprecated adapter and is retained
for historical reference, not as current installation or compatibility guidance.

This adapter provided a Python interface for Zipzi 2D authoring. It
creates and edits 2D model projects, evaluates parameterized geometry, imports
model3/MOC3 input, and exports MOC3 packages. The optional `observe` build adds
offscreen WGPU rendering. Use `zipzi-python2` for current Python SDK development.

The current wheel targets **CPython 3.14** (`>=3.14,<3.15`). The repository
builds and validates wheels, but does not contain a package publication
workflow. Install a wheel built from this repository; do not assume a package
on an index is this build.

## Build and install

For local iteration from the repository root, `./tools/test-sdk.sh python` refreshes
a Debug development install and runs CPU/inspection tests; add `--observe` for
GPU tests. Use `./tools/test-sdk.sh wheel` (or `wheel --observe`) for the packaged
Release acceptance gate. Environments persist under `target/python-tests/`,
and Rust build artifacts reuse the workspace `target/`. Test filters can be
passed after `--`, for example
`./tools/test-sdk.sh python -- -k history`.

From the repository root:

```sh
uv sync --locked
uv build --wheel --python 3.14 --out-dir target/python-wheels crates/zipzi-python
uv pip install --python .venv /absolute/path/to/zipzi-wheel.whl
```

Replace the wheel path with the actual file produced by the build command.
Use `target/python-wheels` for final wheels: Maturin stages its build in
`target/wheels`, so using that same path as `uv build --out-dir` causes a
same-file copy error.
The repository uses uv and the committed root `uv.lock` for Python tools.
Outside the repository, create an environment with `uv venv --python 3.14`
and install the wheel with `uv pip install --python .venv /absolute/path/to/zipzi-wheel.whl`.
After installing a wheel into the repository environment, `uv run --locked`
retains it; an exact `uv sync --locked` removes packages not in the root lock,
so reinstall the wheel if needed.
The workspace release profile disables debug-info stripping only for
`zipzi-python`. This keeps the wheel loadable on macOS 27 with a Homebrew
Rust build linked against an external `llvm@22` whose `llvm-objcopy` still
produces misaligned Mach-O `LINKEDIT` data. Rust upstream [fixed this
alignment bug](https://github.com/rust-lang/rust/pull/158410) in its own LLVM
build. No environment variable is needed when building this repository.
The default build supports CPU authoring and evaluation. For GPU observation,
build with Maturin's `observe` feature and install that wheel instead:

```sh
uv run --locked maturin build --manifest-path crates/zipzi-python/Cargo.toml --release --features observe --out target/python-wheels
```

The GPU build still needs a working graphics device at runtime. Query
`zipzi.capabilities()["gpu_observation"]` before using `zipzi.Observer`.
The default `framework-texture-filtering` feature follows the local Cubism
Native Framework sample: generate source texture mipmaps, use linear filtering
between pixels and mip levels, repeat source UVs, disable anisotropic
filtering, and render with one framebuffer sample. It does
not add MSAA or a postprocess to mesh edges. To build without source mipmaps,
disable default features while retaining `observe`:

```sh
uv run --locked maturin build --manifest-path crates/zipzi-python/Cargo.toml --release --no-default-features --features observe --out target/python-wheels
```

`zipzi.capabilities()["gpu_texture_mipmaps"]` reports which policy the
installed wheel uses.

## First project

Save this as `first_project.py`. Supply an existing PNG and an output directory
using **absolute paths**. The SDK reads the PNG when adding the asset; keep it
available until the project is saved.

```python
from pathlib import Path
from sys import argv
from uuid import uuid4

import zipzi

texture = Path(argv[1]).resolve(strict=True)
output = Path(argv[2]).resolve()
document_id, asset_id, mesh_id = (str(uuid4()) for _ in range(3))

session = zipzi.Session(document_id, origin=(50, 50))
with session.edit("create face") as edit:
    edit.add_png_asset(asset_id, "face texture", texture)
    edit.create_rectangle(mesh_id, "face", asset_id, (40, 40), (60, 60))

print(session.mesh(mesh_id).name)
print(session.evaluate({}).drawables[0].positions)
saved = session.save(output / "project")
print(saved.manifest)  # The actual project manifest path.

reopened = zipzi.open_project(saved.manifest)
print(reopened.mesh_ids())
```

Run `uv run --locked python first_project.py /absolute/texture.png /absolute/output`.
`Session.save()` does not overwrite an unrelated project. For a new numbered
destination when one exists, pass `on_exists="new"` and use the returned
`SaveResult.manifest`.

Models use origin-relative coordinates, X right and Y up. Root geometry and
CPU evaluation use the same model units. `session.source_canvas` retains optional
import provenance; `session.export_settings` independently controls output PPU,
framing and padding. Neither affects geometry or viewport fitting. See the
[coordinate contract](../../docs/archive/coordinates.md).

## Parameterized mesh

Add a parameter and its complete mesh binding in one edit. This example
continues from the project above, before or after saving:

```python
parameter_id, binding_id = str(uuid4()), str(uuid4())
base = session.mesh(mesh_id).positions
shifted = [(x + 10, y) for x, y in base]

with session.edit("add expression") as edit:
    edit.create_parameter(parameter_id, "Open", 0, 1, 0)
    edit.create_mesh_binding(
        binding_id,
        mesh_id,
        [zipzi.Axis(parameter_id, [0, 1])],
        [zipzi.MeshKeyform([0], base), zipzi.MeshKeyform([1], shifted)],
    )

frame = session.evaluate({"Open": 0.5})
print(frame.parameters[0].value, frame.drawables[0].positions)
```

`evaluate()` leaves the session unchanged. Parameter values may use an ID or a
unique display name. If names collide, use IDs. `evaluate_snapshot()` returns
the full drawable and render-plan data; `evaluate()` returns a smaller geometry
view.

## Stable binding conversions

`binding_source()` exposes the native axis/sample IDs and ID-based keyform
coordinates; the existing tuple snapshots remain numeric views. Use stable IDs
when moving samples or changing a grid:

```python
source = session.binding_source(binding_id)["source"]["value"]
sample_id = source["axes"][0]["samples"][1]["id"]
address = session.binding_field_address(sample_id)
plan = session.plan_binding_field_patch(address, {"SampleValue": 0.8})
print(plan.cost)  # combinations, payload, temporary bytes, history estimate
print(plan.affected_ids, plan.losses)
session.execute_binding_plan(plan)
```

Capture a new field address after changing its sampling/driver context. Undo
restores source IDs, but never revives deleted handles or expired addresses.
`plan_binding_authoring()` accepts the shared Rust `BindingAuthoringEdit` enum
shape; `edit.binding_authoring()` composes that same intent with other writes
inside one edit. For example, create an undriven Mesh binding with
`{"Grid": {"Create": {"id": binding_id, "target": {"Mesh": mesh_id}}}}`.
Use `Grid.Edit` with `GridOperation` for axes and ordinary samples, and
`Blend` with `BlendAuthoringEdit` for shared tables/constraints. Plans validate
all shared users, locks, memory limits and the current baseline. Dropping a
plan cancels it without publishing. A stale plan must be rebuilt.

Pending axes are persisted with an empty `parameter_id`; they are excluded
from the runtime grid. Activating the first axis copies the latest base shape.
Clearing/removing an active axis requires an explicit `Slice` or `Resample`
reduction. Shared table base changes compensate every representable user;
constrained or discrete cases that cannot preserve behavior are rejected.
Native projects use directory-project version 21; older experimental native
schemas require reimporting the original source.

## Import and export

CDI display metadata is editable in the same history transaction as model
objects:

```python
with session.edit("CDI labels") as edit:
    edit.create_parameter_group(str(uuid4()), "Face", "顔")
    edit.set_parameter_display_name(parameter_id, "角度")

print(session.display_info(), session.export_cdi3())
```

`edit.import_cdi3(text)` returns diagnostics for unresolved model IDs. The
CDI text is stored with the project, and full package export includes the
generated or imported CDI file.

Expression assets use parameter UUIDs while editing and runtime IDs in exp3:

```python
expression_id = str(uuid4())
with session.edit("smile") as edit:
    edit.create_expression(expression_id, "Smile", [(parameter_id, 0.5, "add")],
                           fade_in=0.2)

print(session.expression_ids(), session.export_expression3(expression_id))
```

`edit.import_expression3(id, name, text)` imports an exp3 file in the current
transaction. It returns diagnostics for parameter IDs absent from the model;
such assets can be saved for repair, but strict exp3 and package export reject
unresolved targets. Package export registers expressions in `model.model3.json`
and writes each exp3 file under `expressions/`.

`preview = session.expression_preview()` captures an independent document
snapshot. Use `preview.schedule_expression(expression_id, 0)`,
`preview.advance(dt)`, `preview.seek(time)`, and `preview.frame()` to inspect
Expression parameter values and evaluated geometry without editing the project.
Create another preview after changing model content.

```python
session = zipzi.Session(str(uuid4()), 100, 100, (50, 50))
result = session.import_model3(Path("/absolute/model.model3.json"))
print(result.moc_version, result.diagnostics, result.warnings)

saved = session.save(Path("/absolute/output/project"))
published = session.export_package(Path("/absolute/output/package"))
print(saved.manifest, published.published, published.warnings)
```

For a standalone MOC3, use `import_bare_moc3(moc3_path, {slot: png_path})` with
an absolute PNG path for each texture slot. Import replaces the session's
current project. Inspect `ImportResult.diagnostics` and
`session.diagnose_resources()` for missing or damaged textures.

Animation assets can be imported or edited inside `session.edit(...)`:
`import_expression3`, `import_motion3`, `import_pose3`, and `import_physics3`
accept source JSON text. Their corresponding `Session.export_*3()` methods
emit runtime JSON; `motion_preview()` combines Motion, Expression, Physics,
and Pose in a detached, seekable CPU preview. `physics_preview()` exposes the
stateful Physics rig alone. See the [API reference](API.md) for timeline and
rig editing methods. Imported missing model3 attachments are reported by
`session.missing_attachments()` and block strict package export until repaired
or explicitly omitted with `edit.discard_missing_attachment(path)`. Managed
Sound and UserData bytes are retained by the project and included in the
exported package.

For layered 8-bit RGB PSD artwork, import into a new project directory:

```python
result = session.import_psd(
    Path("/absolute/art.psd"), Path("/absolute/output/art-project")
)
print(result.manifest, result.raster_layers, result.warnings)
```

The import publishes the project manifest and cropped PNG assets together, then
replaces the session. The destination must not already exist. Unsupported PSD
features fail the import without changing the current session.

Imported PSD layers start as four-vertex rectangles. To add an editable grid
to one layer, including one that already has mesh keyforms, mesh BlendShape
deltas, or corner glue, use:

```python
mesh = session.require_unique_mesh("ArtMesh24")
session.remesh_rectangle_grid(mesh.id, 12, 12)
```

The operation retains corner vertex IDs and migrates existing dependencies in
one undoable edit. Bound meshes require equal column and row counts. For a new
binding created in the same edit, `zipzi.rectangle_grid_geometry()` provides
the grid without opening a separate edit. The one-off
[`Shirousagi experiment`](../../docs/archive/experiments/shirousagi_head_x.py) records
a real-PSD run, including its reference-model displacement projection.

## GPU observation

With an `observe` wheel and `gpu_observation` capability, a saved project can
be rendered as follows:

```python
session = zipzi.open_project(Path("/absolute/output/project"))
with zipzi.Observer(512, 512, 512) as observer:
    frame = observer.observe(session, {})
    frame.save_png(Path("/absolute/output/preview.png"))
    run = observer.observe_run(
        session, [{}], Path("/absolute/output/runs"),
        crop_targets=[zipzi.ObjectRef("mesh", mesh_id) for mesh_id in session.mesh_ids()],
    )
    print(run.report, run.contact_sheet)
```

Add parameter samples to the `observe_run()` list as needed. The call creates
a unique directory containing rendered frames, a contact sheet, and a JSON
report.

## Errors and scripting

SDK validation and IO errors raise `zipzi.SdkFailure`. Inspect `code`,
`operation`, `object_ids`, `field_path`, `expected_version`, `actual_version`,
and `referrers` instead of parsing the message. A failed command aborts its
whole edit, even if the exception is caught inside the `with` block. Leaving
the block with an exception also rolls the edit back. GPU errors raise
`zipzi.ObservationFailure` with `code` and optional `asset_id`.

To run a script once and write a machine-readable result:

```sh
uv run --locked python -m zipzi run /absolute/script.py --report /absolute/report.json
```

The runner records output, exceptions, and session versions. It does not save
projects automatically; scripts must call `Session.save()` themselves.

For method groups, data records, and behavior details, see the
[Python API reference](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python/API.md).
For repository validation, see the
[validation guide](https://github.com/getzipzi/zipzi-2d/blob/main/docs/VALIDATION.md).

## Connect to the resident Editor (R3)

The pure Python client wheel shares this public facade and does not contain or
require `_native`. Build and install it from the repository root:

```sh
python3 tools/python/build_zipzi_client.py
uv pip install --python /absolute/venv target/python-client-wheels/zipzi-0.1.0-py3-none-any.whl
```

Install either this variant or the Maturin native wheel in an environment. Both
use the distribution name `zipzi`; the native wheel also includes the connection
client. Supported Python remains `>=3.14,<3.15`.

```python
from pathlib import Path
from uuid import uuid4
import zipzi

with zipzi.connect(host="/absolute/path/to/zipzi-editor") as connection:
    session = connection.create_session(str(uuid4()))
    # Or: connection.attach_session(connection.list_sessions()[0]["id"])
    parameter_id = str(uuid4())
    with session.edit("Python parameter") as edit:
        edit.create_parameter(parameter_id, "Angle", -30, 30, 0)
        assert edit.parameter(parameter_id).name == "Angle"
    print(session.query_objects(parameters=[parameter_id]))
    connection.show_window(session)
    session.save(Path("/absolute/output/project"))
```

`host` (or `ZIPZI_EDITOR_HOST`) is an absolute executable or `.app` path. With no
explicit path, discovery checks installed Zipzi bundles, including bundle ID
`dev.zipzi.editor`. It never chooses a Debug binary from the current directory.
`start=False` attaches only. The default endpoint is
`/tmp/zipzi-runtime-<uid>/default.sock`; `ZIPZI_RUNTIME_DIR` and
`ZIPZI_RUNTIME_PROFILE` select a shared private absolute directory/profile.
The Editor, desktop reopen, development launcher and Python use the same
ownership lock. The 15-second startup timeout retains the original owner's
attempt; it does not start another host. Closing the connection or exiting Python
keeps formal sessions and history resident. Disconnected candidate edits expire
after a 30-second reconnect grace period.

R3 supports session registration/binding, Mesh/Parameter reads and ordered
batches, candidate reads, mesh creation/rename/positions/deletion, parameter
creation/replacement, commit/cancel, shared Undo/Redo and save/open. Query batches
return detached records from one version. Handshake capabilities report the
available slice; other operations raise `UNSUPPORTED_OPERATION`. Full modeling,
resources, plans, evaluation, animation, independent previews and review follow
in R4–R6; use the native backend for their current APIs.

`zipzi.Busy` confirms that the operation did not run and leaves the Edit writable.
This also applies when another Edit on the same connection has a pending request:
recover that owning Edit first. Each Edit exposes only its own `pending_request`;
it cannot recover another Edit's apply, commit, or Job response.
A retry uses a new request ID automatically. Admission budget failures also retain
the Edit when the server confirms its candidate is still writable. `zipzi.OutcomeUnknown` preserves the
original request and blocks later operations. Keep the Edit and call
`edit.recover()` to reconnect and confirm/replay that same ID; for a non-Edit
operation use `connection.recover()`. `connection.request_status(id)` can inspect
a retained request without consuming its ID or overwriting pending state.
A `with` block propagates unknown outcomes without cancelling or committing again.
Ordinary SDK/conversion errors poison the Edit even when caught. Expired results,
expired clients and restarted hosts return explicit errors; obtain a fresh
connection and inspect formal state before deciding on another write.

Run installed-client subprocess acceptance with
`python3 tools/python/test_connected.py`; optional unittest names select cases.
See the [R3 evidence](../../docs/plans/sdk-runtime/r3-ipc.md) for production desktop
and native Metal acceptance commands.

## R4 modeling API (in progress)

Native and Connected sessions share `modeling_query(**selection)` and
`sample_modeling(parameter_maps)`. Edits also support `apply_modeling(*commands)`,
`create_rgba_image` and `replace_rgba_image`. Batch commands use public SDK record
schemas with an `op` discriminator; the installed
[shared recipe](tests/modeling_recipe.py) shows bindings, BlendShape, candidate
sampling, plans and image persistence on both backends.

`session.plan_modeling(domain, intent)` and the equivalent edit method return an
explicitly owned `ModelingPlan`; apply it with `edit.apply_modeling_plan(plan)`
and release it with `close()` or a context manager. A candidate plan is checked
against its exact edit/sequence. Read-only results include source/version data;
sampling does not change preview values or history.

Connected large payloads use `connection.create_blob(length)`, `append(chunk)`,
`seal()`, `read(offset, length)` and `close()`. Chunks are at most 1 MiB, blobs at
most 256 MiB. `edit.create_rgba_image_from_blob` shares uploaded pixel storage;
`edit.apply_modeling_blob` accepts a sealed UTF-8 JSON command batch. Blob append
response loss keeps its exact request; call `blob.recover()` before continuing.
If creation loses its response, the raised `OutcomeUnknown.blob` exposes the
pending object. Release blobs explicitly after their consuming operation has
been confirmed.

The active protocol is `zipzi-runtime-r4-1`; rebuild/restart old hosts. This first
slice does not yet provide all R4 handles/addresses, IO/export, background jobs
or Editor acceptance. See [R4 status](../../docs/plans/sdk-runtime/r4-modeling.md).
