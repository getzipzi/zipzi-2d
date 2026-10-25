# zipzi Python SDK

Independent typed Python SDK built on the shared Rust Runtime. The Python import
is `zipzi`. All public values, errors and lifecycle wrappers live in this package;
native calls use `LocalHost`, while explicit IPC connections use the same Runtime
handlers. The native bridge owns input arrays before releasing the GIL and has
no JSON transport conversion or direct SDK session owner.

## Layout and builds

| Path | Responsibility |
| --- | --- |
| `client/python/zipzi/` | Pure Python client, values, operation objects and backends |
| `client/pyproject.toml` | Pure `zipzi` distribution; no native dependency |
| `src/lib.rs` | Thin PyO3 adapter; Cargo package `zipzi-python2` |
| `native/zipzi_native/` | Native import namespace |
| `pyproject.toml` | Optional `zipzi-native` distribution |
| `tests/` | Shared Local/Connected recipe and positive/negative type examples |
| `tools/test.py`, `tools/baseline.py` | Source validation and P0 measurements |
| `tools/connected.py` | Pure wheel installation, host startup/session acceptance and Editor's isolated interpreter |
| `tools/generate.py`, `examples/authoring_schema.rs` | Compiled Rust contract and selected mesh/resource/Plan generation |
| `client/python/zipzi/_generated/`, `src/generated/` | 37 managed outputs, including uniform facades, backend bindings, metadata, public reexports and the offline API reference |
| [CAPABILITIES.md](CAPABILITIES.md) | Explicit scope and delivery evidence |

From the repository root, using the workspace's CPython 3.14 tools:

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2 PythonSdk.test_shared_recipe
./tools/test-sdk.sh python2 --release

# Two independently built distributions:
uv build --wheel --out-dir target/python2/client-wheels crates/zipzi-python2/client
uv build --wheel --out-dir target/python2/native-wheels crates/zipzi-python2
```

The source test runner builds only this adapter and the CPU Runtime fixture,
copies the native artifact into `target/python2/native`, runs a shared recipe in
isolated Python processes, and checks actual valid/invalid public type examples.
It uses macOS extension-module/dynamic-lookup linking, so CPython symbols come
from the importing interpreter. Tests never load Cargo's mutable output in place.

The package currently targets CPython `>=3.14,<3.15`; source acceptance is on
macOS/Unix. Other platform/version claims require separate CI evidence.

## P3A target-contract baseline

P3A-0 has a frozen symbol map, signatures, experiment configuration and Release
baseline. P3A-1 is accepted with complete-call recovery, cached Edit state/receipt
and known Job timeouts; this round's final performance noise blocker was waived by
the user. See the [acceptance record](../../docs/plans/python-sdk-rewrite/p3a-1.md). Remaining target regressions in `tests/test_p3a_contract.py`
continue separately while later P3A tasks are implemented. After refreshing fixtures
with `./tools/test-sdk.sh python2`, run from the repository root:

```sh
uv run --locked python tools/python/python2_contract.py baseline --output target/python2/p3a-baseline.json
./tools/test-sdk.sh python2 --recovery
```

This command preserves every failure and returns a nonzero status until the
selected contracts pass. Use a new output path for every run. Inventory/freeze
commands and their limits are documented in [tools](../../tools/README.md).
P3A-2 has completed result unification, domain-owned operations, generated DTOs
and common plan/apply. Pagination, generated facades and documentation/agent gates
continue in P3A-3–6. The examples below describe the current API.

## Selected code generation

P2 derives mesh, image and Plan values/operations, codecs, query signatures and
simple convenience methods from selected Rust declarations. Public imports stay
domain modules such as `zipzi.meshes`/`zipzi.parameters`, root `PlanCost`/`PlanReport` and the static `zipzi.ops` unions; lifecycle, resource ownership and recovery stay
handwritten. Local conversion remains direct PyO3, with no JSON roundtrip.

```sh
uv run --locked python crates/zipzi-python2/tools/generate.py --check
uv run --locked python crates/zipzi-python2/tools/generate.py
uv run --locked python tools/test_tools.py -p test_python2_codegen.py -v
```

The source runner checks generated drift before building. Generated files have
explicit ownership markers; the generator cannot overwrite handwritten targets.
Normal wheels consume checked-in outputs and do not run generation at import or
installation. The paired source archives carry the same Python outputs; the native
sdist also carries the SDK dependency closure, Rust declarations, schema exporter
and generator. Its build backend removes reviewed desktop-only workspace patches
and preserves locked SDK dependency versions. It delegates wheel builds to Maturin.

```sh
uv run --locked python crates/zipzi-python2/tools/distribution.py \
  --output target/python2/source-acceptance
```

This checks generation from extracted sources, rebuilds both normal wheels, and
runs isolated Local/Connected recipes. See the [integrated acceptance record](../../docs/plans/sdk-runtime/r1-r3-python-p1-p2-acceptance.md).

## First model

After installing both local wheels:

```python
from pathlib import Path
import zipzi
from zipzi import meshes, ops

with zipzi.local() as runtime:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000001")
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

For a running host, select `zipzi.connect(endpoint=absolute_socket_path)` instead.
Without an endpoint, `connect()` discovers the environment's runtime directory/profile.
All reads use `zipzi.Snapshot[T](source, value)` or `zipzi.Batch[T](source, items)`.
Domain payloads (`meshes.Mesh`, `parameters.Parameter`, `bindings.MeshBinding`,
`resources.Image`, `scene.Summary`) have no nested Source. Empty batches retain
their exact source. CPU samples use `Snapshot[sampling.Result]`; canonical modeling
records, capabilities and Editor context also use `Snapshot`. Field access sends
no requests. `Page`/`PageCursor` define the result shape; collection pagination
remains P3A-4 work. A failed
write makes its candidate uncommittable even when caught. Admission refusal is
distinct from execution failure.

Jobs must be waited or closed to release their bounded result slots. Their actual
result is cached locally. `close()`/context exit waits for entered work. A lost
response raises `OutcomeUnknown`; its `recover()` returns the complete original
method's runtime result: creation returns an ID and commit returns `Job[Receipt]`.
For exact static types use `zipzi.recover_call(edit.meshes.create, spec)` (or another
registered bound SDK request method). It invokes the method once and recovers
once; business functions/lambdas are rejected before execution. Recovery uses
the original immutable request bytes and request ID. There is no `Edit.recover`.

`edit.state: EditState` and `edit.receipt: Receipt | None` read cached confirmed
state without IO. A caught unknown outcome is raised again on normal context
exit; normal exit requires confirmed commit or explicit cancellation. Ordinary
write/conversion errors still abort the candidate. `job.wait(timeout=None)` waits
until terminal; a finite nonnegative timeout raises `JobTimeout`, retains the
known accepted Job and does not cancel it. Transport loss remains OutcomeUnknown.

## Images and source-bound plans

```python
from zipzi import resources

with runtime.resources.prepare_image(1, 1, bytes((12, 34, 56, 255))) as image:
    with session.edit("Texture") as edit:
        asset = edit.resources.create_image("Draft", image)
        with edit.plan(resources.RenameImage(asset, "Texture", expected_name="Draft")) as plan:
            report = plan.report  # Immutable cached value; no request.
            edit.apply(plan)
        pixels = edit.resources.get(asset).value.pixels
```

Preparation owns an independent lease. Closing it after creating or replacing an
image leaves candidate, formal and history references intact. Use
`edit.resources.replace_image(asset, prepared)` to retain the resource identity
and name while changing its pixels. Pixel snapshots own immutable `bytes`;
`runtime.resources.usage()` reports Runtime-wide lease/plan counts and tracked
Rust image buffers (not process RSS).

`session.plan(operation)` and `edit.plan(operation)` accept the registered subset
of the same domain operations used by `edit.apply`: mesh/image renames, scene
operations and the eight SDK planning domains under `modeling.ModelingCommand*`.
For example, `meshes.Rename(id, "Final", expected_name="Before")` works in both
paths. The optional label precondition is checked by the SDK; omitted conditions
still bind planning to the exact captured Source. `resources.RenameImage` has the
same contract. `ops` exports only `Operation` and `PlannableOperation`.

Apply a Plan with `edit.apply(plan)` or as the first item of
`edit.apply_many((plan, operation, ...))`. A batch accepts at most one Plan;
invalid ordering aborts the candidate before execution. Formal Plans require an
initial candidate of the exact session/version; candidate Plans require the exact
Edit and sequence. Stale/foreign/closed Plans cannot publish partial changes.
The cached `PlanReport` remains readable after close. `plan_async` retains the
registered modeling subset and returns `Job[Plan]`.

Image preparation, reads, Plan creation and handle close also retain their
original recovery owner after a lost response. Call `OutcomeUnknown.recover()`;
another action cannot consume that result. Close/context releases handles only
after a confirmed outcome. Runtime shutdown or client expiry releases remaining
leases. This first control-transport slice limits RGBA payloads to **1 MiB**;
binary buffers and larger images belong to P4.

The current native/IPC pairing is listed under Complete Runtime R4 routes below.
Connected hosts must advertise
`typed_meshes`, `typed_resources_plans`, `typed_resource_values`, `typed_job_lifecycle`, `typed_parameters`,
`typed_event_pages`, `typed_mesh_batch`, `typed_mesh_bindings` and `typed_modeling_samples`; a mismatched host is rejected during
connection.

`job.state()` reads status without consuming the result. `job.cancel()` requests
cancellation; queued work reports `CANCELLED`, while already entered IO reports
its actual result. `wait()`/`close()` cache both terminal success and failure.
Recover a lost cancel/status response before waiting or taking another action.

Explicit Connected shutdown releases unsubmitted candidates, prepared images and
Plans while retaining formal sessions. Entered Jobs finish on the host. Closing
with another unresolved request raises Busy. Explicit
`runtime.close(abandon_pending=True)` disconnects and forfeits recovery; cleanup
follows host expiry without a new write ID. A lost close
acknowledgment has its own recovery owner.

The lifecycle matrix is recorded in the
[P1 acceptance table](../../docs/plans/python-sdk-rewrite/p1-lifecycle-matrix.md).
Fault gates are compiled only under `test-support`; normal wheels expose neither
fault controls nor allocator probes.

## Desktop host and resident sessions

```python
with zipzi.connect(start=True, host=Path("/absolute/path/to/zipzi-editor")) as runtime:
    metadata = runtime.connection  # Immutable host PID/runtime/build/capabilities/limits.
    for info in runtime.sessions.list():
        session = runtime.sessions.attach(info.id)
        session.show_window()
```

`start=False` is the default. Discovery follows `ZIPZI_RUNTIME_DIR` and
`ZIPZI_RUNTIME_PROFILE`, matching the Rust host. Startup transfers an already
locked endpoint descriptor to a `--no-window` host; a timeout leaves any running
owner intact. Protocol/build mismatch never launches a fallback host. Pass
`sdk_build=` to pin an expected SDK source identity. Recovering a request never
starts a new host. `host=` or `ZIPZI_EDITOR_HOST` chooses an executable; macOS
also searches the user/system `Applications/Zipzi Editor.app` locations.

`SessionId(runtime, serial)` is a pure immutable identity; `session.id` sends no
request. `sessions.list()` and `sessions.attach(id)` work on both backends.
`SessionInfo` includes state/path/version/history. `show_window()` requires a
desktop host. The new client's minimal R3 connection is implemented; full R4 and
packaged application location acceptance remain pending.

```sh
./tools/test-sdk.sh python2-connected
# Optional current production desktop host, in addition to CPU fault acceptance:
./tools/test-sdk.sh python2-connected --editor-host "$PWD/target/debug/zipzi-editor"
```

The pure wheel runs from `/tmp` under `-I`, without a native package. Editor's
focused `runtime_ipc_acceptance` also uses this installed client. See the
[reconnection record](../../docs/plans/sdk-runtime/python2-r0-r4.md) for the
coverage and remaining R4 modeling work.

## Typed parameters and events

```python
from zipzi import events, parameters

stream = session.subscribe()
with session.edit("Angle") as edit:
    angle = edit.parameters.create(parameters.ParameterSpec("Angle", -30., 30., 0.))
    edit.parameters.patch(angle, parameters.ParameterPatch(name="Angle X"))
    values = edit.parameters.get_many((angle,))
    faces = edit.meshes.get_many((face,))
for event in stream.poll():
    if event.kind == "published":
        print(event.receipt.after)
```

Create/patch operations compose in the same explicit batch and candidate. Rust
owns defaults, identity and range/dependency validation; omitted patch fields
preserve the current candidate's values. Reads own immutable source-bound values.
`parameters.list()` uses SDK storage order. Both parameter and mesh batches admit
the complete selection before cloning (at most 256 records and 1 MiB of
conservative response estimates); large/paged collections remain R4 work.

`runtime.sessions.subscribe()` reads registry metadata; `session.subscribe()`
reads session metadata. Each captures its initial information and cursor together
without cloning a model. Polling is independent per reader, defaults to 128
events and permits at most 256. A gap requires explicit `resync()`; an unresolved
poll must first recover its original response. Cursors and event values contain
no native objects. Use `runtime.events_since(cursor)` to continue on another
connection to the same host. Subscriptions do not drain SDK events or allocate a
server lease. See [R3 evidence](../../docs/plans/sdk-runtime/python2-r3-events.md).

## Mesh bindings and CPU samples

```python
from zipzi import bindings, sampling

with session.edit("Bind face") as edit:
    binding = edit.bindings.create(bindings.MeshBindingSpec(face,
        (bindings.Axis(angle, (0., 1.)),), (
            bindings.Keyform((0.,), geometry.positions),
            bindings.Keyform((1.,), tuple((x + 10., y) for x, y in geometry.positions)))))
    source = edit.bindings.get(binding).source
    candidate = edit.modeling.sample(({angle: .5},), expected=source,
        checks=sampling.Checks(min_triangle_area=.01))
formal = session.modeling.sample(({angle: 0.}, {angle: .5}, {angle: 1.}))
```

Axis/sample identity allocation, complete-grid validation, interpolation and
clamping remain in Rust core/SDK. Keyform appearance defaults come from SDK;
`set_keyform` replaces one numeric grid point while preserving axis identities.
Binding snapshots expose canonical sample IDs and immutable positions/appearance;
`get/get_many/list` admit at most 256 records and a conservative 1 MiB result.
Sampling returns exact Source/Version, requested/evaluated parameters, CPU
frames (drawables/deformers/offscreens/render commands) and advisory geometry
diagnostics. It creates no publication, history entry or Preview change.
Up to 64 poses with 256 explicit parameters each are admitted before capturing a
frozen source. Geometry-based expansion admission and the remaining aggregate host budget can
require smaller batches. `sample_async` and `plan_async` capture an immutable
source and run on cancellable read workers; formal publication retains a worker
slot. Scene bindings and BlendShape use the same SDK evaluator. The P3 recipe additionally verifies mesh BlendShape
interpolation and propagation through bound topology changes. Both backends use the same Runtime entry; Local
conversion uses direct owned PyO3 values. Generation covers the selected mesh/resource/Plan and P3 scene types; these later binding and sampling types remain handwritten.

## P3 scene modeling and SDK identities

`zipzi.scene` provides generated, typed operations for Parts, parameter groups,
Warp/Rotation creation, organization, local reparenting, mesh BlendShapes,
vertex/triangle edits, Glue pairs and Offscreen keyforms/mapping. Use
`edit.apply` / `apply_many`; these operations share the existing candidate,
source sequence, failure state and commit. Defaults and identity allocation stay
in SDK/core. `session/edit.scene.get/get_many` return immutable metadata with an
exact Source; geometry and explicit CPU poses use the mesh/sampling queries.
Metadata batches admit at most 256 selections and a conservative 1 MiB response
before copying; a query refusal does not poison an Edit.

```python
from zipzi import scene

with session.edit("Organize") as edit:
    folder = edit.apply(scene.CreatePart(scene.PartSpec("Face"))).ids[0]
    edit.apply(scene.MoveToPart((face,), part_id=folder))
    value = edit.scene.get(face).value
    with edit.plan(scene.SetSceneLabel(face, value.name, "Final")) as plan:
        edit.apply(plan)

with session.reference("mesh", face) as reference:
    source = reference.resolve()
    capabilities = reference.scene_capabilities()
    with reference.label() as address:
        with address.plan("Reviewed") as plan:
            with session.edit("Review label") as edit:
                edit.apply(plan)
```

Scene plans retain their exact formal/candidate source and immutable cost/loss/
copy reports; local reparenting selects SDK `KeepLocalData`. A label address
captures SDK identity, owner/context and field epochs. Moving preserves a handle;
delete/Undo never revives it. A move, field change or Undo invalidates the old
address even when the final value matches. Addresses remain usable after their
parent reference is closed. References and addresses share a 1024-slot per-client
limit and count against host CPU estimates; close/client expiry/session close
release their tables. Every create/read/plan/close retains its own recovery owner.

## Complete Runtime R4 routes

The native pairing contract is `zipzi-python2-14`; IPC is `zipzi-runtime-p3a-345-1`.
Rebuild both adapters and hosts after contract changes. Compiled Rust serde
shapes generate `zipzi.modeling`, native constructors/writers and the binary
array codec. `edit.apply` accepts the full `ModelingCommand` family, including
SDK rectangle remeshing, binding conversions, topology dependencies, metadata
and attachments. `modeling.records(ModelingQuery(...))` returns selected canonical
records with an exact formal/candidate source. No complete model mirror is kept
by the client. Large mesh/binding/record/sample results use immutable binary
blobs; small ordinary getters retain a single RPC.

`session.address(selector)` and `reference.capabilities(domain)` preserve the
SDK field/context epochs and incarnation checks. `session.capabilities(query)`
provides collection capabilities. Plans and read jobs retain their original
version and candidate sequence; close/release frees their ownership.

`runtime.resources.prepare_buffer(bytes)` returns a reusable immutable Buffer;
`prepare_image(width,height,bytes_or_buffer)` returns an independent PreparedImage
lease. `load_png(path)` and `prepare_png(buffer)` return cancellable Jobs.
`session/edit.resources.snapshot(id)` pins the original encoded bytes and pixels,
including disk images, until the ImageLease is closed. Preparation does not
require an active Edit. Image create/replace joins the normal atomic batch.

`runtime.sessions.import_(path, format)` imports PSD, Model3, bare MOC3 (explicit
texture slots) or Cubism into a new session. `session.project.replace/open` and `run(ProjectRequestNew(...))`
changes generation through an explicit Job; `save`, `diagnose` and `export`
return typed receipts/reports. Export covers package, CMO3, CAN3, paired Cubism,
Motion3, Expression3, Physics3, Pose3 and CDI3 under existing SDK loss/format rules.
Reports preserve diagnostics, warnings, publication and durability. The optional
`session.editor_context()` is versioned and returns no context in a headless host.

Admission precedes input expansion, source cloning, decode, blob growth and seal
peaks. Default host accounting is 2 GiB; image/PNG/blob allocations share 768 MiB,
with a 512 MiB blob table. A binary payload is at most 256 MiB, transferred in
1 MiB chunks. Recovery deliberately retains up to 256 MiB of binary backing per
client (512 MiB across clients), for at most 300 seconds; a completed download
can therefore leave a bounded cache. Distinct resource allocations are charged
once even when candidates, history, jobs or leases share them. These are
conservative admission bounds and actual payload counters, not process RSS.

R4 verification and commands are tracked in the
[complete acceptance record](../../docs/plans/sdk-runtime/python2-r4-complete.md).
The later Python [P3A public-contract refactor](../../docs/plans/python-sdk-rewrite/p3a-api-agent-contract.md)
remains a separate stage.

## P0 measurements

```sh
./tools/test-sdk.sh python2 --release
./tools/test-sdk.sh python2 --release --instrumented
uv run --locked python crates/zipzi-python2/tools/baseline.py \
  --output target/python2/baseline-first
```

The first measurements cover selected queries, single writes, 100-operation
batches, large position arrays, 256×256 RGBA preparation/create/commit/release,
and formal Plan creation/apply/commit/release. Python JSON/native boundary counts and Python
heap probes are measured in separate processes and instrumented binaries from
latency. Both the adapter and CPU host use frozen Release artifacts. Schema 2
records actual Rust codec calls, requested Rust heap live/peak bytes, selected
explicit payload copies, and array materialization. Copy scope excludes implicit
domain clones and allocator-internal copies; heap scope excludes Python/C heaps,
allocator overhead and realloc transient storage. Connected probes retain an
empty control-window calibration. Noise and regression policies are recorded;
CPU fixture numbers do not claim Editor latency.

Animation, independent Preview and observer/inspection remain later Runtime capabilities. See the
[construction plan](../../docs/plans/python-sdk-rewrite-plan.md) and
[implementation record](../../docs/plans/python-sdk-rewrite/README.md).


## P3A-3/4/5 implementation (acceptance deferred)

This implementation is staged for P3A-6. By user request, no behavior, type,
installed-package, Editor, performance or agent acceptance was run in this slice.
The generator was run and the native adapter was compiled while implementing.

All five object collections now provide `get(id, expected=...)`,
`get_many(ids, expected=...)` and `list(name=None, limit=128, cursor=None)` on both
Session and Edit. List returns `Page[Summary]` in SDK storage order, with exact
case-sensitive names and an opaque source-bound continuation. Restart explicitly
on `STALE_SOURCE`; an empty page can still advance a bounded scan. Metadata
never includes geometry/keyforms/pixels. Resources also support selected batches.

`runtime.capabilities()` and `runtime.limits()` query host facts. Sessions can
`detach()` or `close(discard_modified=False)`; aliases share binding state,
modified/other-client/active-work refusals leave the document intact. Creation
accepts an omitted document UUID allocated by Rust.

`zipzi.describe("Session.meshes.list")` reads the installed offline directory.
The [short guide](client/python/zipzi/docs/guide.md),
[generated API reference](client/python/zipzi/docs/reference.md) and
[executable recipes](client/python/zipzi/docs/recipes.py) are included as package
resources. Pure DTO codecs and internal ownership factories use private names.
Owned DTO and query-container repr/summary is bounded and does no IO.

Generation uses `schema/authoring.json`; API bindings, public reexports, complete
request registration and transport-hook metadata come from the same finite
binding layer. Rust field docs survive export into class docstrings/reference.
Runtime state machines, identity rules, resource ownership and binary transfer
stages remain handwritten. New normal native/IPC pairs are required; no fallback.

For the later focused run, `./tools/test-sdk.sh python2 --discovery` selects the
staged regressions. Agent packets and recorders are prepared via
`tools/python/python2_agent_tasks.py --help`; preparing fixtures does not run an
agent or prove G01–G08. Full doc/typing coverage, fault-controller integration and
independent task execution remain part of the later combined acceptance.
