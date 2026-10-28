# Zipzi Python SDK: agent workflow

Import `zipzi` and use `zipzi.describe("zipzi")` to discover the installed API.
Use `zipzi.describe("Session.meshes.list")` for one method at a time. The reference
and recipes here match this installed client.

`print(zipzi.describe("modeling.ModelingQuery").format())` prints complete help,
including fields and child paths. Direct printing, `repr` and `summary` are bounded
previews. Describing or formatting help sends no host requests.

## Choose an execution host

`with zipzi.local() as runtime:` explicitly owns a process-local Runtime and needs
the matching optional native wheel. `zipzi.connect(endpoint=...)` uses the existing
host; `start=True` explicitly allows starting one. Installing native code never
changes an established connection. Pure imports and `describe` require neither a
native wheel, a host nor a GPU.

Query `runtime.capabilities()` and `runtime.limits()` before selecting work. These
describe current availability without reserving admission. Local has no Editor
windows. `runtime.connection` caches handshake information.

## Select and read

Use `runtime.sessions.list()` and attach an explicit `SessionId`. Create with
`runtime.sessions.create()` when the task needs a new document; shared Rust code
allocates its persistent UUID. A freshly initialized document is unsaved and
modified; save it or explicitly discard it when closing. Never guess an existing object's UUID.

Every collection has `list(name=None, limit=128, cursor=None)`, `get(id, expected=None)`
and `get_many(ids, expected=None)`. A name filter is case-sensitive and exact.
Duplicate names are legitimate: use ownership and IDs to choose the intended
object. List returns small metadata, excluding geometry, keyforms and pixels.
Follow `next_cursor` until None. A page may contain fewer than limit items or even
be empty after a bounded scan; continue while a cursor exists.

A cursor binds its collection, filter and exact Source. A formal change, Undo/Redo
or candidate sequence change causes `STALE_SOURCE`: explicitly restart from the
first page. Do not concatenate pages from different sources. A recovered lost
reply returns the original captured page, including its original continuation.

Read `snapshot.value` or `batch.items`; source is on the outer container, including
empty results. `get_many` preserves order and duplicate IDs and fails as a whole
for missing IDs or budget refusals. Queries do not poison an Edit. Explicit
`expected=source` rejects a changed source atomically on the host.

### Choose mesh records and follow references

`meshes.get/get_many` returns identity, name and geometry. Use canonical
`modeling.records` for texture references, Part/deformer membership, appearance,
draw order and other authoring metadata. Requested record slots may be `None`
when an ID is absent. This example assumes an attached `session` and a known
`mesh_id`; it keeps subsequent image reads on the same Source:

```python
from zipzi import modeling

snapshot = session.modeling.records(modeling.ModelingQuery(meshes=(mesh_id,)))
mesh = snapshot.value.meshes[0]
if mesh is not None:
    part_id = mesh.part_id or None
    parent_id = mesh.deformer_id or None
    print(part_id, parent_id, mesh.appearance, mesh.draw_order)
    texture = mesh.texture_asset_id
    if isinstance(texture, modeling.SceneReferenceResolved):
        image = session.resources.get(texture.object_id, expected=snapshot.source)
        print(image.value)
    elif isinstance(texture, modeling.SceneReferenceUnresolved):
        print("Texture reference is unresolved:", texture)
    else:
        print("No texture reference was specified")
```

Canonical `Mesh.part_id/deformer_id` and `Part.parent_id` use `""` for the root
or an unassigned relationship. Scene summaries use `None`. Convert canonical
values with `value or None` when comparing to summaries; use `value or ""` when
constructing canonical records. Resolved references expose `.object_id`, ordinary
objects `.id`, and typed `PartId`/`TransformId` wrappers `.value`; unresolved
references must not be treated as existing object IDs.

`SessionInfo.id` and `Source.session` identify a Runtime session;
`SessionInfo.version.session_id` and `Source.version.session_id` identify the SDK
authoring session. Do not compare their serial numbers as if they share an ID
namespace. `SessionInfo.path` is a project path, not an Editor display title;
there is currently no SessionInfo display-name field. A pathless session remains
selectable by its Runtime identity. Optional `editor_context()` alone does not
guarantee a unique active document across windows.

Convenience `edit.meshes.create` and `edit.parameters.create` return IDs directly.
`edit.apply(scene.CreatePart(scene.PartSpec("Face")))` returns a `WriteResult`;
the created Part ID is in `.ids[0]`. Organization membership and drawing groups
are independent: `MoveToPart` does not promise to change draw-group membership.
Inspect canonical drawing groups as well as the evaluated order when reorganizing.

Convenience `bindings.Axis.keys` and `bindings.Keyform.keys` use numeric parameter
coordinates. Canonical binding axes contain samples with stable UUIDs; canonical
keyforms refer to those sample identities. Match numeric coordinates to the axis
samples before constructing canonical keyforms; a numeric key is not a sample ID.

### Mesh quality scope

The Python modeling API currently exposes rectangle remeshing, manual topology
edits and complete topology replacement, but no Alpha contour meshing entry.
The Rust SDK's Alpha mesher is not yet bound to Python. Topology replacement
requires explicit dependency/binding and vertex-mapping handling; do not assume
arbitrary remeshing automatically migrates every keyform, BlendShape or Glue.

Sampling checks cover small triangles, inconsistent winding and model bounds.
An empty diagnostic list says nothing about unsampled poses, minimum angles,
texture contour fit or animation stretch. GPU observation provides appearance
images without wireframe/vertex/constraint overlays. For topology inspection,
read geometry at the same Source and draw it separately using its documented
coordinate space; evaluate key poses and intermediate poses explicitly.

## Edit and plan

Use `with session.edit("meaningful history label") as edit:`. Candidate reads see
prior successful operations. Construct operations in their domain (`meshes.Rename`,
`parameters.Patch`, `bindings.Create`, `resources.CreateImage`, scene/modeling).
`ops.Operation` and `ops.PlannableOperation` are type unions, not constructor namespaces.
Ordinary convenience methods construct these same operations.

Geometry triangles contain vertex IDs, not offsets into positions. Arrays follow
the corresponding vertex-ID order; UV and model coordinates retain SDK semantics.
Inputs are copied into immutable owned collections before execution. Canonical
modeling records use their documented SDK fields; no Python model mirror exists.

`session.plan(operation)` plans against formal content. `edit.plan(operation)`
plans against that exact candidate/sequence. Inspect report cost and losses before
`edit.apply(plan)`. A batch can contain one Plan, as its first item, followed by
operations. A later write invalidates an older candidate Plan. Keep its report
for inspection; close the Plan after use. Address plans also bind identity,
context and field epochs, so recreating an ID cannot revive an old address.

Normal Edit context exit commits and waits. The confirmed `edit.receipt` states
whether content changed and identifies before/after versions. Explicit cancel
leaves receipt None. Ordinary write or conversion failures make the candidate
uncommittable even if caught: fix inputs and start a fresh Edit. Confirmed Busy
or pre-execution budget refusals preserve the candidate; act on recovery_action.

## Continue work in the Editor

Attach the desktop Session. `edit.commit().wait()` updates its in-memory document
and adds one Undo step when changed. Editor views refresh; people can refine the
content and the next Edit reads their changes. Script exit retains committed
resources; saving is separate. Use the installed example:

```sh
python -I -m zipzi.docs.editor_handoff list
python -I -m zipzi.docs.editor_handoff rename \
  --runtime RUNTIME_UUID --session SESSION_SERIAL --mesh MESH_UUID --name "Refined face"
```

Use `--endpoint /absolute/path/to/profile.sock` before the subcommand for a
specific Unix host. Windows uses a local Named Pipe name. Prefer discovery with
`ZIPZI_RUNTIME_DIR` (an absolute directory) and `ZIPZI_RUNTIME_PROFILE` (a short
lowercase name); `zipzi.connect(start=True, host=absolute_host_path)` uses those
settings. Windows currently supports the CPU Runtime fixture; desktop Editor
handoff remains pending. Installing the pure client does not install or launch
a host unless `start=True` is explicitly requested.

## Recover and wait

Use `zipzi.recover_call(edit.meshes.create, spec)` when static result typing matters.
Only registered SDK methods are accepted; it calls the method once and can recover
the original Call once. It never retries arbitrary business code or an entire workflow.

On `OutcomeUnknown`, retain the original exception/owner and call `error.recover()`.
The recovered runtime result has exactly the original public return type. Do not
resend the write, create a new request identity or infer success from silence.
Recovery can remain unknown; keep the new exception. Resolved old exceptions cannot
consume newer requests. Recover commit into a Job, then wait for its Receipt.

`JobTimeout` means the accepted Job remains pending. It does not mean cancellation,
failed execution or unknown write outcome. Keep the Job and wait again. `cancel()`
is a request: entered IO may still finish and reports its real outcome. `wait()`
caches success/failure and releases the result slot. `close()` waits, rather than
cancels. Pending status/cancel requests must be recovered first.

## Verify, save and release

After commit, reread objects and verify content, Source, values, pixels or history.
Bounded repr and `summary()` perform no RPC or array/byte expansion.

Prepare images independently with `runtime.resources.prepare_image`; preparation
is not a document write. Accept or replace through an Edit. Prepared images,
Buffers, Plans, references and pixel leases have explicit ownership; close them.
Candidate/formal/history owners retain accepted image data independently.

Animation authoring uses typed canonical commands through the same Edit and
Plan contract. Motion clips, registrations, tracks/keys/events, expressions,
Physics and Pose records can be selected without changing playback or history:

```python
from uuid import uuid4
from zipzi import modeling as m

group_id = str(uuid4())
with session.edit("Animation group") as edit:
    edit.apply(m.ModelingCommandMotion(
        intent=m.MotionEditCreateGroup(id=group_id, name="Idle")))
snapshot = session.modeling.records(m.ModelingQuery(motion_groups=(group_id,)))
assert snapshot.value.motion_groups[0].name == "Idle"
```

Empty animation ID lists request no payloads; missing selected IDs produce None
slots. Set `physics=True` or `pose=True` to select those singleton payloads.
`session.plan/plan_async` and `edit.plan/plan_async` accept the four domain edit
commands. Field addresses retain native identity/context/field epochs: editing a
field and undoing it does not revive the old address. Large curves use the same
bounded binary transfer and original-call recovery as mesh data.

`session.preview()` or `edit.preview()` captures an independent frozen animation
source. Create a new Preview for new authored content; commit/cancel does not
refresh an existing one. Controls use generated, keyword-only values:

```python
from zipzi import animation

with session.preview() as preview:
    preview.apply(animation.SetParameter(id=parameter_id, value=.5))
    result = preview.sample((0., .25, .5)).wait()
    frame = result.value.frames[-1]
    assert frame.preview_id == preview.id
    assert result.source == preview.source
```

ScheduleMotion/ScheduleRegistration/ScheduleExpression use absolute activation
times, which cannot precede the current SDK clock. Reset returns to zero and
retains schedules. Activations start on the first update at/after their time;
use `frame.motion.motion_positions` for actual local sample coordinates.
`seek(time)` and `advance_to(time)` return a one-frame batch Job; `sample(times)`
returns 1..64 frames, retaining order/duplicates/backward targets. Geometry is
optional (`geometry=False`). Model opacity remains a separate SDK channel.

Pending work refuses changes to that Preview. An interrupted SDK seek preserves its
prior state; cancellation after a completed batch reports its success, while a cancelled batch can retain completed targets, and incremental
advance can retain completed steps. Cancelled output is discarded. Close the
Preview to release its lease and cancel pending work, then wait/close the Job.
Other clients/views, Editor playback and authored history are unaffected.

Use `modeling.spatial` for frozen formal/candidate geometry, or pass `queries` to
Preview sampling to query its exact animated frame, even with `geometry=False`:

```python
from zipzi import spatial

queries = (spatial.Hits(point=(0., 0.), details=True),)
result = session.modeling.spatial(({},), queries).wait()
hits = result.value.frames[0].results[0].value
```

Bounds select Mesh/Part UUIDs (Parts include descendants). Coordinates are Y-up
model units. Hits return descending SDK draw order, total/truncation, and optional
triangle indices, stable vertex IDs, barycentric weights and UVs. These are geometric
candidates; textures, masks and occlusion are not pixel-tested. Hidden/disabled
objects require `include_hidden=True`. Up to 64 poses and 64 queries, 4096 total
target selectors and 4096 retained hit candidates per query are accepted, subject
to Runtime byte budgets. Order and duplicate poses/queries are preserved.

GPU observation runs explicitly on workers; imports and CPU queries never probe
a device. Use a model-space ROI with `observation.View`:

```python
from zipzi import observation, inspection

view = observation.View(width=256, height=256, roi=(0., 0., 1., 1.))
images = session.modeling.observe(({},), view).wait()
report = inspection.save(images, output_directory)
```

Preview sampling accepts `view=view` and applies SDK Model opacity. Each batch
resolves textures once and returns RGBA8, PNG, source/texture hashes and actual
view mapping. Missing/changed resources fail explicitly. Cancellation is checked
between capture/render units; entered decoding/GPU work can finish. Images need
no close. `capabilities().gpu` is None before probing, then caches success or
adapter/device failure; later renders can still fail. `inspection.save` writes
PNG and JSON locally into a new directory, even after Runtime closure.

Save with `session.project.save(absolute_host_path).wait()` and inspect `durable`
and the receipt version. Paths refer to the execution host, not the client machine.

`session.detach()` invalidates all aliases in this client and retains the host
session; active work refuses detach. `session.close()` actually frees the session,
path lock and attached resources. It refuses unsaved changes or other clients.
Save first, or use explicit `discard_modified=True` only when the task permits it.
Attach again to create a new binding; old references stay invalid. Close Runtime
last. Connected close releases the client while formal sessions remain resident;
Local close releases the Runtime's sessions. Unknown pending work defaults to Busy;
`abandon_pending=True` explicitly forfeits recovery and never authorizes replay.
