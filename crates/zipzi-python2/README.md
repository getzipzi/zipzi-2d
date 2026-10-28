# zipzi Python SDK

Typed Python client for the shared Rust Runtime. The `zipzi` distribution is
pure Python; `zipzi-native` supplies the adapter for `zipzi.local()`.
`zipzi.connect()` uses a resident desktop host and does not require the native
package. Both backends use the same Runtime operations.

The packages target CPython 3.14. Source tests run on macOS/Unix. See
[capabilities](CAPABILITIES.md), the [guide](client/python/zipzi/docs/guide.md),
[API reference](client/python/zipzi/docs/reference.md) and
[recipes](client/python/zipzi/docs/recipes.py). The installed package also offers
`zipzi.describe("zipzi")` for offline discovery.

## Build and test

Run from the repository root:

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2 --recovery
./tools/test-sdk.sh python2 --discovery
./tools/test-sdk.sh python2-connected
# GPU observation requires a usable GPU.
./tools/test-sdk.sh python2 --observation

uv build --wheel --out-dir target/python2/client-wheels crates/zipzi-python2/client
uv build --wheel --out-dir target/python2/native-wheels crates/zipzi-python2
```

`python2-connected` installs the pure client and tests host discovery and session
binding. Add `--editor-host /absolute/path/to/zipzi-editor` to test a desktop host.

On Windows, run the SDK gates with `uv run --locked python tools/dev.py test sdk python2`
and `uv run --locked python tools/dev.py test sdk python2-connected`. Local uses a
native `.pyd`; Connected uses Named Pipes. See the [Windows plan](../../docs/plans/windows-development.md)
for platform validation.

## First model

After installing both local wheels:

```python
from pathlib import Path
import zipzi
from zipzi import meshes

with zipzi.local() as runtime:
    session = runtime.sessions.create()
    geometry = meshes.Geometry(
        (42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
        ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),),
    )
    with session.edit("Face") as edit:
        face = edit.meshes.create(meshes.MeshSpec("Face", geometry))
        snapshot = edit.meshes.get(face)
        edit.meshes.set_positions(face, vertex_ids=snapshot.value.geometry.vertex_ids,
                                 positions=((2., 3.), (3., 3.), (2., 4.)))
        edit.apply(meshes.Rename(face, "Final"))
    saved = session.project.save(Path("/absolute/output/project")).wait()
```

## Sessions and edits

`zipzi.connect()` discovers a resident host; pass `endpoint=` to select a socket
or `start=True, host=Path(...)` to launch an Editor. `runtime.sessions.list()` and
`sessions.attach(info.id)` find and attach existing sessions. `show_window()`
requires a desktop host. Edits commit to the shared document and history; saving
the project is a separate operation.

Reads return `Snapshot[T]` (`source`, `value`) or `Batch[T]` (`source`, `items`).
Collection `list()` returns source-bound pages; follow `next_cursor` and restart
on `STALE_SOURCE`. Plans and asynchronous reads retain their captured source.
A failed write aborts its candidate. Jobs should be waited or closed to release
resources; a finite `job.wait(timeout=...)` can raise `JobTimeout` without
cancelling the accepted job.

A lost response raises `OutcomeUnknown`. Recover the original call with its
`recover()` method, or use `zipzi.recover_call(bound_sdk_method, ...)` for a
typed call. Do not repeat the operation as a new request. An Edit context must
finish with a confirmed commit or explicit cancellation. Close an unresolved
Runtime only after recovery or with `abandon_pending=True`.

The client supports modeling, resources, project import/export, animation,
independent previews, CPU spatial queries and GPU observation. Operations are
organized by domain (`meshes`, `parameters`, `bindings`, `scene`, `resources`
and `modeling`). Use `session.plan` or `edit.plan` to inspect supported operations
before applying them. Image preparation and snapshots own explicit leases; close
them when no longer needed.

## Code generation

Generated domain values and bindings come from Rust declarations and the API
schema. Lifecycle, ownership and recovery are handwritten. Regenerate after
contract changes and rebuild the native adapter and host together.

```sh
uv run --locked python crates/zipzi-python2/tools/generate.py --check
uv run --locked python crates/zipzi-python2/tools/generate.py
uv run --locked python tools/test_tools.py -p test_python2_codegen.py
```

## Editor mesh generation (Connected only)

`zipzi.mesh_generation.generate(runtime, width, height, alpha, settings=..., stage=...)`
uses the GPL implementation in the Editor process; the MIT Runtime and Python
client do not link it. A host advertises `editor_mesh_generate` when available.
Local hosts and older Editor hosts reject this optional operation explicitly.
The result is an owned `meshes.Geometry`; generation never modifies a document.
Use an Edit to create a mesh or the topology replacement workflow to replace one.
Positions are bottom-up source pixels, so transform them into the target model's
coordinate system before publishing. UVs are normalized; input alpha is top-down.

```python
import zipzi
from zipzi.mesh_generation import MeshSettings, generate

width = height = 64
alpha = bytes(255 if 8 <= x < 56 and 8 <= y < 56 else 0
              for y in range(height) for x in range(width))
with zipzi.connect(timeout=60) as runtime:
    settings = MeshSettings(edge_spacing=12, interior_spacing=16)
    contour = generate(runtime, width, height, alpha, settings=settings, stage="edges")
    mesh = generate(runtime, width, height, settings=settings,
                    stage="interior", geometry=contour)
    # Or generate both stages together with stage="full" (the default).
```

This synchronous endpoint is bounded to 4194304 source pixels, 65534
output vertices and 131068 input triangles. Larger inputs are rejected rather than resized. Generation may hold the Editor
request loop while computing. All five fill strategies and three edge
layouts are supported. Settings are independent of the UI's texture-scaled defaults.
Socket timeouts retain the normal same-request recovery behavior.

Focused production-host acceptance: `./tools/dev-python crates/zipzi-python2/tests/editor_mesh_generation.py --host /absolute/path/to/zipzi-editor`.
