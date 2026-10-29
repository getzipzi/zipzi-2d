# Zipzi Python SDK

Build and animate Zipzi 2D models from Python, automate authoring workflows,
and collaborate with people or AI agents in the Zipzi Editor. The typed `zipzi`
API supports modeling, animation, project I/O, previews and inspection.

## Installation

Zipzi requires **CPython 3.14**. Install the client with pip:

```sh
python -m pip install --pre zipzi
```

The `--pre` flag allows alpha releases. For an in-process Runtime, install the
native backend too:

```sh
python -m pip install --pre zipzi-native
```

`zipzi-native` installs its matching `zipzi` client as a dependency. Native
wheels are prepared for macOS arm64 and Windows x86-64. The pure Python client
can connect to a compatible running Editor without `zipzi-native`. Package
installation from PyPI is available once the relevant release is published; see
[GitHub Releases](https://github.com/getzipzi/zipzi-2d/releases) for release
artifacts and the desktop Editor.

## Create a model

This example uses the local Runtime and requires `zipzi-native`:

```python
from pathlib import Path

import zipzi
from zipzi import meshes

with zipzi.local() as runtime:
    session = runtime.sessions.create()
    geometry = meshes.Geometry(
        (42, 7, 99),
        ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0)),
        ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0)),
        ((42, 7, 99),),
    )
    with session.edit("Create face") as edit:
        edit.meshes.create(meshes.MeshSpec("Face", geometry))

    # Use an absolute path that the Runtime can access.
    session.project.save(Path("/absolute/path/to/project")).wait()
```

An Edit commits changes to the document and its history; saving the project is a
separate operation. Jobs such as save must be waited for or closed.

## Connect to the Editor

Start the Zipzi Editor, then discover and attach to one of its open sessions:

```python
import zipzi

with zipzi.connect() as runtime:
    for info in runtime.sessions.list():
        print(info.id, info.path)
        # session = runtime.sessions.attach(info.id)
```

Choose the session you want to edit by its ID. Changes made through an Edit
appear in the shared document and undo history, so a person can continue in the
Editor and a script or agent can continue from their changes. Saving is separate.
`zipzi.connect()` can also select an endpoint or start an explicitly specified
host; see the [usage guide](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/client/python/zipzi/docs/guide.md).

Connected Python and Editor versions may differ when they share the stable wire
protocol major (`zipzi-runtime-v1`). Compatible API additions and optional host
services do not require a new handshake version. Earlier `rN` protocols need a
coordinated upgrade to this baseline. See the
[compatibility rules](../zipzi-runtime/README.md#local-protocol).

## Explore the API

- [Usage guide](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/client/python/zipzi/docs/guide.md): sessions, edits, collaboration, recovery and resource ownership.
- [API reference](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/client/python/zipzi/docs/reference.md) and [recipes](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/client/python/zipzi/docs/recipes.py): signatures and executable workflows.
- [Capabilities](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/CAPABILITIES.md): available operations and host requirements.

The installed package also includes the guide, reference and recipes. Discover
API entries without a host or network connection:

```python
import zipzi

print(zipzi.describe("zipzi").format())
print(zipzi.describe("Session.meshes.list").format())
```

Call `runtime.capabilities()` and `runtime.limits()` to inspect what the
connected host supports. If a response is lost, recover the original call with
`OutcomeUnknown.recover()` or `zipzi.recover_call()` instead of submitting it
again. A `JobTimeout` does not cancel the job; wait again or close it. See the
guide for details.

## Editor mesh generation

A compatible connected Editor can generate mesh geometry from alpha data. This
optional service runs in the Editor and does not modify the document itself:

```python
import zipzi
from zipzi.mesh_generation import MeshSettings, generate

width = height = 64
alpha = bytes(
    255 if 8 <= x < 56 and 8 <= y < 56 else 0
    for y in range(height)
    for x in range(width)
)
with zipzi.connect(timeout=60) as runtime:
    geometry = generate(
        runtime, width, height, alpha,
        settings=MeshSettings(edge_spacing=12, interior_spacing=16),
    )
```

Check for `editor_mesh_generate` in `runtime.connection.capabilities` before
using it. The service
is unavailable on local or older hosts and enforces input and output limits.
Positions are bottom-up source pixels and UVs are normalized; input alpha is
top-down. Transform positions into the target model's coordinate system before
creating or replacing a mesh. See the [capabilities guide](https://github.com/getzipzi/zipzi-2d/blob/main/crates/zipzi-python2/CAPABILITIES.md).

## Contributing

From the repository root, run `./tools/test-sdk.sh python2` for the Python SDK
checks or `./tools/test-sdk.sh python2-connected` for installed-client and host
discovery checks. See [development and test coverage](https://github.com/getzipzi/zipzi-2d/blob/main/tools/README.md#test-coverage)
and the [release guide](https://github.com/getzipzi/zipzi-2d/blob/main/docs/RELEASING.md)
for source builds and packaging.

The Python client and Runtime components are MIT licensed. Editor-provided
services run in the GPL-3.0-only Editor; see the [licensing guide](https://github.com/getzipzi/zipzi-2d/blob/main/LICENSING.md).

### Dependency-cleaning deletion

`session.plan` / `plan_async` and their Edit equivalents accept
`modeling.ModelingCommandDeletion`. The request uses typed Mesh, Part, Deformer,
Glue or Offscreen roots. Part means its organization subtree; Deformer means its
deformation subtree regardless of the caller's tree view. `DissolvePart` and
`RemoveDeformer` retain their children and report required migrations.

```python
from zipzi import modeling as m

operation = m.ModelingCommandDeletion(intent=m.DeletionIntent(
    request=m.DeletionRequest(roots=(m.DeletionTargetMesh(value=mesh_id),))))
with session.plan_async(operation).wait() as plan:
    summary = plan.report.deletion
    page = plan.deletion_page(m.DeletionSection.REWRITTEN, limit=64)
    # Read further pages with offset=page.next_offset, and inspect effects,
    # decisions, locks and exports as needed. Owned axes/samples/keys have a
    # separate section; the summary does not copy the model's object inventory.
    if summary.state in (m.DeletionReviewState.READY,
                         m.DeletionReviewState.NEEDS_CONFIRMATION):
        with session.edit("Delete and clean dependencies") as edit:
            edit.apply(plan)
```

A blocked or unresolved request still returns an inspectable Plan. Resolve its
reported choices in a new request and replan. New opaque export failures require
explicit acceptance of those exact typed resource failures; accepting them keeps
extensions and provenance and does not make later export succeed. A Plan applies
only to its exact source, including the Edit identity and sequence when captured
inside an Edit. Its report pages remain fixed when the live source changes, and
are readable until the Plan lease closes. Page sizes are 1–256 rows. Directly
applying a deletion intent returns `DELETION_PLAN_REQUIRED`.
