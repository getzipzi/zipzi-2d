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
host; see the [usage guide](client/python/zipzi/docs/guide.md).

Connected Python and Editor versions may differ when they share the stable wire
protocol major (`zipzi-runtime-v2`). Compatible API additions and optional host
services do not require a new handshake version. Earlier `rN` protocols and
`zipzi-runtime-v1` need a coordinated upgrade to this baseline: contract
extraction changes Rust type identities and binary signatures. See the
[compatibility rules](../zipzi-runtime/README.md#local-protocol).

## Explore the API

- [Usage guide](client/python/zipzi/docs/guide.md): sessions, edits, collaboration, recovery and resource ownership.
- [API reference](client/python/zipzi/docs/reference.md) and [recipes](client/python/zipzi/docs/recipes.py): signatures and executable workflows.
- [Capabilities](CAPABILITIES.md): available operations and host requirements.

The installed package includes a [README](client/python/zipzi/README.md), guide,
reference and recipes with local links. Locate them without a host or network
connection:

```python
from importlib.resources import files

package = files("zipzi")
print(package.joinpath("README.md").read_text(encoding="utf-8"))
print(package.joinpath("docs/guide.md"))
```

Discover API entries offline:

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
creating or replacing a mesh. See the [capabilities guide](CAPABILITIES.md).

## Contributing

From the repository root, run `./tools/test-sdk.sh python` for the Python SDK
checks or `./tools/test-sdk.sh python-connected` for installed-client and host
discovery checks. See [development and test coverage](../../tools/README.md#test-coverage)
and the [release guide](../../docs/RELEASING.md)
for source builds and packaging.

The Rust contract exporter supplies a normalized type graph to the Python
generator. `schema/authoring.json` indexes the checked domain shards in
`schema/types/`; the generator also maintains `generated-manifest.json` for
its owned outputs. After changing a contract or projection, regenerate and
verify from the repository root:

```sh
uv run --locked python crates/zipzi-python/tools/generate.py
uv run --locked python crates/zipzi-python/tools/generate.py --check --no-cache
```

Public modeling types live in `zipzi.modeling.<domain>`, while the generated
values, codecs, binary adapters and native bridges are organized by domain.
For a packaged schema replay, add `--schema path/to/schema/authoring.json` to
the verification command. The replay validates each shard and the complete
contract digest before rendering.

The Python client and Runtime components are MIT licensed. Editor-provided
services run in the GPL-3.0-only Editor; see the [licensing guide](../../LICENSING.md).

### Dependency-cleaning deletion

`session.plan` / `plan_async` and their Edit equivalents accept
`modeling.commands.ModelingCommandDeletion`. The request uses typed Mesh, Part, Deformer,
Glue, Offscreen, Parameter, ParameterFolder, Texture and Attachment roots, or
`DeletionTargetRecord` for typed business and child identities. Part means its
organization subtree; Deformer means its deformation subtree regardless of the
caller's tree view. `DissolvePart` and
`RemoveDeformer` retain their children and report required migrations.

```python
from zipzi import modeling as m

operation = m.commands.ModelingCommandDeletion(intent=m.deletion.DeletionIntent(
    request=m.deletion.DeletionRequest(roots=(m.deletion.DeletionTargetMesh(value=mesh_id),))))
with session.plan_async(operation).wait() as plan:
    summary = plan.report.deletion
    page = plan.deletion_page(m.deletion.DeletionSection.REWRITTEN, limit=64)
    # Read further pages with offset=page.next_offset, and inspect effects,
    # decisions, locks and exports as needed. Owned axes/samples/keys have a
    # separate section; the summary does not copy the model's object inventory.
    if summary.state in (m.deletion.DeletionReviewState.READY,
                         m.deletion.DeletionReviewState.NEEDS_CONFIRMATION):
        with session.edit("Delete and clean dependencies") as edit:
            edit.apply(plan)
```

Parameters reduce ordinary axes at their frozen document default. Override one
with `DeletionRequest(parameter_slices={parameter_id: value}, roots=...)`.
`ParameterFolder` retains contents; `ParameterFolderContents` explicitly deletes
the folder subtree and its parameters. Texture users need an explicit entry in
`texture_replacements`: a retained texture UUID or `None` to leave them
unconfigured. Optional attachment references clear. Motion/Expression/Physics and
metadata cleanup share the same atomic Plan and paged report. Physics node and
Blend base sample choices use `physics_node_replacements` and
`blend_base_replacements`. These fields describe fixed authoring inputs and do
not read another client's preview sliders or playback state.

A blocked or unresolved request still returns an inspectable Plan. Resolve its
reported choices in a new request and replan. New opaque export failures require
explicit acceptance of those exact typed resource failures; accepting them keeps
extensions and provenance and does not make later export succeed. A Plan applies
only to its exact source, including the Edit identity and sequence when captured
inside an Edit. Its report pages remain fixed when the live source changes, and
are readable until the Plan lease closes. Page sizes are 1–256 rows. Directly
applying a deletion intent returns `DELETION_PLAN_REQUIRED`.

### Maintaining Python reference documentation

The generator derives signatures, properties, aliases and enum members from the
Python AST. Shared type, field and enum-variant semantics come from Rust contract
`///` comments, exported by `ApiShape`, `ApiFields`, `api_record!` and `api_union!`.
Maintain Python-specific method usage beside its definition: docstrings for
handwritten APIs, and the corresponding descriptor in `tools/generate_api.py`
for generated convenience methods. For example, each mesh entry in `WRITES`
contains its signature, execution expression and usage documentation together.
Page introductions live in `client/python/zipzi/docs/reference-intros/*.md`;
these fragments are included in the generated reference pages. API aliases such
as `Point` use attribute docstrings immediately following their definitions.
Generated reference pages and offline `zipzi.describe()` share the same records.

Run `uv run --locked python tools/test_tools.py -p test_python_codegen.py -k reference`
for documentation structure and link checks, then regenerate and verify outputs
with the commands above. Missing field/callable semantics and example targets
fail generation. Shared owned-value ownership and IO conventions are documented
once in `docs/reference/common.md`, sourced from `CompactValue`.

Type summaries are never filled with generic ownership text. Missing summaries
remain empty while their signatures and field descriptions remain available.
Each generation and `--check` prints coverage counts; the generated
`client/python/zipzi/docs/type-documentation.json` report groups missing summaries
by type, lists public aliases and identifies the Rust or Python declaration.
Field documentation alone does not count as a type summary.

Run the strict semantic gate with:

```sh
uv run --locked python crates/zipzi-python/tools/generate.py --check-docs
```

This checks without writing and returns a nonzero status while any type summary
is missing. Existing gaps are exposed in the report; `--check` continues to
verify reproducibility separately from semantic coverage.

Type references are resolved from Python annotations and import bindings into
public API paths in the shared `type_refs` record. Reference pages declare
external types once using standard Markdown reference links and the shortest
unambiguous page-local label. Types defined on the same page use their existing
API headings. `zipzi.describe(path).type_refs` exposes the full public paths
without recursively loading their help.

Collection and candidate-write rules live beside `_facade_support.Collection`
and `_lifecycle.Edit`. Generator descriptors bind their stable rule identities
before rendering. Markdown links to each rule in `reference/common.md`;
docstrings contain complete help, and `ApiDescription.format()` expands only
the entry's directly referenced leaf rules. Fixed discriminator roles come from
variant projections, so business fields such as `ParameterPatch.kind` retain
their own semantics. Entries have no default recipes example; module navigation
still links to the guide and recipes.

`docs/documentation-quality.json` separately lists missing enum/union branch
semantics, unresolved SDK annotation references, placeholders and a callable
parameter/return review checklist. `review_required` means the text still needs
semantic review; `not_applicable` is distinct from a missing item. Nonempty type
summaries alone do not establish branch or callable completeness.

Run the staged reference gates with:

```sh
uv run --locked python tools/test_tools.py -p test_python_codegen.py -k PythonReference
```

The tests inventory every routed page by rollout batch, check accepted pages
and exact cross-page pilot paths, and compare stable source/projection debt
against `tools/python/python-reference-debt.json`. Remove resolved debt from
that baseline when fixing documentation; the test rejects both new gaps and
stale baseline entries, so one fix cannot hide a regression elsewhere. It stores
identities and diagnostic categories, never business descriptions. Unreviewed
domains remain visible in both reports until their own batch is accepted.
