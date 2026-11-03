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

Connected compatibility uses the stable wire major `zipzi-runtime-v2`, separately
from package versions and source build hashes. Compatible API additions and
different optional capabilities do not prevent connection. Check
`runtime.connection.capabilities` before choosing optional services; unsupported
calls return `UNSUPPORTED_OPERATION` and leave the connection usable. Binary
layout signatures are still checked for the affected calls. An explicit
`zipzi.connect(sdk_build=...)` additionally pins an exact build; the default does
not. Earlier feature-specific `rN` protocols require upgrading Python and Editor
to this baseline together. `zipzi.local()` still needs its matching native wheel.

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
    if isinstance(texture, modeling.scene.SceneReferenceResolved):
        image = session.resources.get(texture.object_id, expected=snapshot.source)
        print(image.value)
    elif isinstance(texture, modeling.scene.SceneReferenceUnresolved):
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

### Mesh quality

See the [mesh quality and inspection guide](mesh-quality.md) for sampling, overlays and coverage.

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
settings. The Windows Editor supports connected sessions; desktop validation
is still in progress. Installing the pure client does not install or launch a
host unless `start=True` is explicitly requested.

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

See the [animation, spatial queries and observation guide](animation-spatial.md) for these workflows.

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
