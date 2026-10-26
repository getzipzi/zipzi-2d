# Zipzi Python SDK: agent workflow

Import `zipzi` and use `zipzi.describe("zipzi")` to discover the installed API.
Use `zipzi.describe("Session.meshes.list")` for one method at a time. The reference
and recipes in this directory belong to this exact client contract. Historical
repository plans are not usage documentation.

## Choose an execution host

`with zipzi.local() as runtime:` explicitly owns a process-local Runtime and needs
the matching optional native wheel. `zipzi.connect(endpoint=...)` uses the existing
host; `start=True` explicitly allows starting one. Installing native code never
changes an established connection. Pure imports and `describe` require neither a
native wheel, a host nor a GPU.

Query `runtime.capabilities()` and `runtime.limits()` before selecting work. These
are actual host facts and do not reserve future admission. Local has no Editor
windows or GPU observer. `runtime.connection` is cached handshake information.

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

After confirmed commit, reread the intended objects and compare content, Source,
numeric values, pixels or history against the task. Bounded repr and `summary()`
perform no RPC and avoid expanding arrays/bytes; explicitly access fields when needed.

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
bounded binary transfer and original-call recovery as mesh data. Independent
animation Preview/playback and GPU observation are not available in this slice;
native object preview eligibility does not imply a host Preview API is present.

Save with `session.project.save(absolute_host_path).wait()` and inspect `durable`
and the receipt version. Paths refer to the execution host, not the client machine.
A client-side file existence check does not prove a remote host saved durably.

`session.detach()` invalidates all aliases in this client and retains the host
session; active work refuses detach. `session.close()` actually frees the session,
path lock and attached resources. It refuses unsaved changes or other clients.
Save first, or use explicit `discard_modified=True` only when the task permits it.
Attach again to create a new binding; old references stay invalid. Close Runtime
last. Connected close releases the client while formal sessions remain resident;
Local close releases the Runtime's sessions. Unknown pending work defaults to Busy;
`abandon_pending=True` explicitly forfeits recovery and never authorizes replay.
