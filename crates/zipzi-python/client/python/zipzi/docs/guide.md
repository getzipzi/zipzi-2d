# Authoring with Python

This guide and [executable recipes](recipes.py) ship with the installed client.
Requires CPython 3.14. Install `zipzi` for an Editor connection; install the
matching `zipzi-native` as well for Local execution.

New users: start with [installation and first run](getting-started.md), then
[ownership and lifetimes](lifecycle.md). This guide focuses on task workflows.

## Find an API without starting a host

Browse the [generated API reference](reference/index.md) for methods and
overloads, read-only response fields, and domain types with constructor defaults
and enum members. The reference is included in the installed package:

```python
from importlib.resources import files

print(files('zipzi').joinpath('docs/reference/index.md').read_text(encoding='utf-8'))
```

For focused interactive help:

```python
import zipzi

print(zipzi.describe().format())
print(zipzi.describe('Session.query').format())
print(zipzi.describe('values.Command').format())
print(zipzi.describe('values.CommandCreateParameter').format())
print(zipzi.describe('results.SaveReceipt').format())
```

`children` contains paths for further queries. Pass `next_offset` as `offset`
to continue an entry's children; the default page contains 32 paths. Use
`describe('values')` to browse all generated inputs and domain values, and
`describe('results')` for response shapes. `help(Type)` and `inspect.signature`
work too. No discovery call starts an Editor or imports the native adapter.

The value package is divided into domains such as `values.parameters`,
`values.meshes`, `values.bindings`, `values.motion`, `values.physics`,
`values.commands` and `values.operations`. Browse one with
`describe('values.meshes')`. Existing `from zipzi import values as v` code works;
focused imports are also available:

```python
from zipzi.values.meshes import Mesh, MeshGeometry
from zipzi.values.motion import MotionClip
```

`v.Mesh` and `Mesh` are the same immutable type, with the same codec and defaults.

Generated constructors validate fields and own immutable nested values. Their
`repr` is a short preview; use `.to_wire()` to inspect complete content. Field
semantics come from the shared contract. Response mappings and tuples are also
immutable. `results` declares their read-only keys for IDEs and type checkers.

## Choose a host and document

`with zipzi.local() as runtime:` owns a native in-process host. Closing it releases
its documents without saving. `with zipzi.connect() as runtime:` connects to a
running Editor. `endpoint=Path(...)` selects a private endpoint explicitly;
`start=True, host=Path(...)` permits starting an Editor executable when needed.
The client and host must support wire major v4. Check `runtime.capabilities()`
and `runtime.limits()` before selecting optional services.

List documents before editing an existing one:

```python
with zipzi.connect() as runtime:
    for info in runtime.sessions.list():
        print(info['session'], info['path'], info['dirty'])
```

Attach an explicit ID from that output with `runtime.sessions.attach(session_id)`.
Multiple windows, absent paths and duplicate names are possible; do not assume
the first document is the intended one. `runtime.sessions.create()` creates a new
unsaved document. `session.detach()` releases your attachment and retains the
document. `session.close()` closes it and checks unsaved changes and other
attachments. Discarding edits requires `discard_modified=True`.

## Query typed values and follow pages

The following snippets assume an attached `session` and explicitly selected IDs.
The recipes provide complete functions and a runnable Local CLI.

```python
from zipzi import values as v

page = session.query(v.QueryCollection(
    kind=v.ObjectKind.PARAMETER, offset=0, limit=128,
))
for parameter in page.records(v.Parameter):
    print(parameter.id, parameter.name, parameter.minimum, parameter.maximum)
if page.value['next_offset'] is not None:
    following = session.query(v.QueryCollection(
        kind=v.ObjectKind.PARAMETER, offset=page.value['next_offset'], limit=128,
    ), source=page.source)
```

Continue until `next_offset` is `None`. The `list_parameters` recipe implements
the complete loop. Reuse the original source for all pages and dependent reads.
If another client publishes, `STALE_SOURCE` means discard collected pages and
start again; do not combine pages from different sources. Names may be duplicated:
select by stable ID after inspecting ownership and references.

`QueryObjects(addresses=(ObjectAddress(...), ...))` preserves requested order and
duplicates. Missing IDs fail the whole query with `MISSING_OBJECT`; at most 256
records may be requested. Collection page sizes are 1–256. Queries return full
records, which can include geometry; request a small page for large objects.
`QuerySummary()` returns identity and counts. `QueryCollectionCapabilities(...)`
returns current creation/move/reorder verdicts; availability does not reserve a
later operation.

`Snapshot.source` distinguishes formal and candidate content; `content` identifies
immutable content within the resident session/history. It is not a persistent
file hash: reopening or importing creates a new content identity even when the
authored values are equal. To verify a saved/reopened project, compare stable
object IDs, fields, geometry and pixels rather than comparing `content` with an
earlier save receipt. `snapshot.records(v.Mesh)` decodes a homogeneous selection into
generated Mesh values. For mixed kinds, inspect each `address` and decode its
`value` with the corresponding `Type.from_wire(...)`. Root references in some
canonical domain records use the empty string; unresolved imported references
are explicit variants and must not be treated as existing object IDs.

Mesh creation takes `MeshCreateSpec.geometry` and a texture asset ID string.
A queried `Mesh` exposes `vertex_ids`, `base_positions`, `uvs` and `triangles`
directly; it has no `geometry` attribute. Its texture is a typed reference:

```python
mesh, = session.query(v.QueryObjects(addresses=(
    v.ObjectAddress(kind=v.ObjectKind.MESH, id=mesh_id),
))).records(v.Mesh)
if isinstance(mesh.texture_asset_id, v.SceneReferenceResolved):
    print(mesh.texture_asset_id.object_id)
```

Compare these fields directly. `repr` is a bounded diagnostic preview, so do not
parse it to resolve references or compare complete geometry.

## Read, edit and verify

```python
selection = v.QueryObjects(addresses=(
    v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter_id),
))
snapshot = session.query(selection)
assert isinstance(snapshot.source, v.QuerySourcePublished)
with session.draft('Rename parameter', expected=snapshot.source.value) as draft:
    draft.apply(v.CommandRename(target=selection.addresses[0], name='Head angle'))
    parameter, = draft.query(selection).records(v.Parameter)
    assert parameter.name == 'Head angle'
```

This publishes once and creates one undo step. Formal Session reads keep seeing
the resident publication while Draft queries see the candidate. Candidate
commands advance `draft.source.sequence`. A concurrent publication makes a Draft
stale: inspect the current document and rebuild the intended edit.

`draft.apply(*commands)` submits an atomic batch; an ordinary accepted command
failure ends the Draft. `draft.preview(*commands)` prepares a gesture proposal;
rejection retains the last valid candidate. Normal Draft context exit publishes,
exceptions cancel. `OutcomeUnknown` retains ownership for recovery instead.
`draft.apply_async(...)` makes the Draft busy: wait for its Job before further
commands or leaving the Draft context. Undo/Redo advance revision even when
restoring earlier content; old sources do not become valid again.

## Save, import and export

Editing and saving are separate. An explicit path is absolute on the host:

```python
from pathlib import Path

with session.save(Path('/absolute/path/model.zipzi')) as job:
    receipt = job.wait()
print(receipt['published'], receipt['durable'], receipt['warnings'])
```

`published` reports destination replacement. Check `durable` and `warnings`
for storage-sync issues even after publication. `session.save()` uses the current
project path. `fingerprint=` supplies an exact destination precondition.

`save(path)` without a fingerprint creates a new destination and returns
`PROJECT_EXISTS` if it already exists. After the first save, `session.save()`
updates that same tracked project using its known fingerprint. To replace a
different known destination intentionally, supply the fingerprint from its
earlier save receipt:

```python
fingerprint = v.ProjectFingerprint.from_wire(previous_receipt['fingerprint'])
with session.save(destination, fingerprint=fingerprint) as job:
    receipt = job.wait()
```

An open project path is owned by its resident Session. Opening the same file in
a second Session on that host returns `PROJECT_BUSY`. To verify a saved file,
reopen it in its existing Session, or close that owner before opening another.
Detaching retains the resident Session and its project ownership.

`session.open(path)` replaces content from a native project. `import_project(path,
v.ImportFormat.PSD)` imports PSD; Model3 and Cubism are also supported. Both return
Jobs and reset history on replacement. Save or explicitly resolve unsaved work
first. `session.export(path, v.ExportFormat.RUNTIME)` exports captured content;
read its receipt for durability and conversion notices. Use `allow_loss=True`
only when the intended conversion may discard unsupported data.

## Own jobs and bulk bytes

Use context managers for Jobs and Blobs. Each `append` or `read` transfers at most
1 MiB. Image uploads contain RGBA bytes, exactly `width * height * 4`; seal before
`draft.replace_image_blob`. The `create_scene` recipe demonstrates a complete
image and mesh edit. `read_all` handles multi-chunk readback.

`session.samples(v.SampleRequest(...))` returns a Job whose Blob contains UTF-8
JSON: source, content and ordered frames. `sample_animation` accepts clip IDs,
activation times and sample times in seconds. For activation scheduled at the origin,
pass `motions=((clip_id, 0.0),)`; the second value is its start time, not a weight.
Empty motions samples the rest pose. Sampling is
read-only and does not add undo history.

Animation sampling uses preview playback, with canonical 1/60-second update
ticks. An activation starts on its first eligible update; the clip's local time
can differ from the requested absolute time. For an activation scheduled at zero,
positive seeks start playback at the first tick (1/60 second); a sample at time
zero shows the initial key. Compare curve values using each frame's
`animation['motion_positions'][i]['local_time']`, not the requested time.
This preserves the Editor playback behavior.

The `create_linear_motion` recipe shows all fields required for a real parameter
track and clip, including fades and extension maps:

```python
from zipzi.docs.recipes import create_linear_motion, sample_animation

clip_id = create_linear_motion(session, parameter_id)
report = sample_animation(session, (0., .25, .5, .75, 1.), motions=((clip_id, 0.),))
for frame in report['frames']:
    animation = frame['animation']
    print(animation['time'], animation['motion_positions'], animation['parameters'])
```

For this -20..20 linear sweep, the expected value is `-20 + 40 * local_time`.
Keep the authored parameter default unchanged when sampling a motion.

A host with `gpu_observation` supports `session.observe(input, view)`. Its Blob is
a sequence of packets: four-byte big-endian JSON-header length, that header,
then `png_bytes` bytes of PNG. The first header includes the shared texture report.
The `observe` and `observation_packets` recipes decode complete packets.

## Handle timeouts and lost responses

A `JobTimeout` leaves work running. Catch it **inside** the Job context to wait
again; leaving the context closes the Job and cancels unfinished work:

```python
with session.save() as job:
    try:
        receipt = job.wait(timeout=1)
    except zipzi.JobTimeout:
        receipt = job.wait()
```

`OutcomeUnknown` means the mutation may have happened. Recover the original call
and preserve the returned result:

```python
draft = session.draft('Rename')
try:
    outcomes = draft.apply(command)
except zipzi.OutcomeUnknown as error:
    outcomes = zipzi.recover_call(error)
# The recovered apply has updated this same Draft's source.
draft.publish()
```

Recovery can itself lose a response: recover the new `OutcomeUnknown` before
continuing. Keep the Runtime, Draft and Job owners alive until resolution.
Never replay a mutation under a fresh request ID. Recovering a lost context-exit
publication does not resume the Python `with` block; explicitly inspect the
recovered outcome and release resources when finished.

For event readers, `session.subscribe()` returns a starting cursor. Keep a cursor
per reader; use `session.events(cursor)` and advance to the final event's version.
`EVENT_GAP` requires a new subscription and fresh state inspection.
