# Animation, spatial queries and observation

Animation authoring uses typed canonical commands through the same Edit and
Plan contract. Motion clips, registrations, tracks/keys/events, expressions,
Physics and Pose records can be selected without changing playback or history:

```python
from uuid import uuid4
from zipzi import modeling as m

group_id = str(uuid4())
with session.edit("Animation group") as edit:
    edit.apply(m.commands.ModelingCommandMotion(
        intent=m.motion.MotionEditCreateGroup(id=group_id, name="Idle")))
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

## Observation texture budgets

Observation defaults to original source pixels. Select a smaller logical base
RGBA budget explicitly; changing screenshot dimensions alone does not resize
textures:

```python
from zipzi import observation as o

view = o.View(width=1024, height=1024, roi=(0., 0., 10., 10.),
              texture_policy=o.FitBudget(max_rgba_bytes=128 << 20))
estimate = session.modeling.estimate_observation(({},), view)
if estimate.static_feasible and estimate.runtime_available:
    result = session.modeling.observe(({},), view, expected=estimate.source).wait()
    report = result.value.texture_report
    print(report.source_rgba_bytes, report.render_rgba_bytes, report.resized_count)
```

Edit provides the same methods on its candidate. For animation use
`preview.estimate_observation(times, view=view, geometry=False)` before
`preview.sample(times, view=view, geometry=False)`. Estimates read metadata without
image IO, PNG encoding, animation replay, GPU initialization or task reservation.
A busy Preview rejects estimates; reading an estimate leaves its clock and ordinal
unchanged. CPU-only hosts can estimate and report their missing GPU support.

`Original()` fails if referenced source RGBA exceeds the host ceiling.
`FitBudget` requires a positive integer at most
`runtime.limits().observation_texture_rgba_bytes`; booleans, strings and dictionaries
are rejected. Every referenced asset counts separately, including hidden/mask
sources; unused images do not enter the texture budget. Budgets below four bytes
per referenced asset return an insufficient-budget estimate. Invalid metadata or
policy returns an error. Execution never lowers the requested screenshot size.

The budget covers base RGBA only. Mips, one-source decode/downsample workspace,
GPU uploads/rendering, output and source/result ownership have separate accounting.
Host limits expose their bounds; estimates are conservative logical counts, not
RSS measurements. Source/device verification flags remain false until execution;
a successful estimate does not guarantee later capacity or device support.
Source freezes the document/candidate, while external files can still change.
Execution validates their dimensions and declared hashes without adapting its plan.

Each batch contains one `texture_report` with policy, sorted source/render sizes,
verified typed source identities, render pixel hashes and algorithm versions.
Preview without a View has no report. Source-alpha quality remains based on the
original pixels even when rendering uses thumbnails. Equivalent rendering inputs
have the same render digest regardless of policy/budget; source scene digests do
not change with thumbnail size. `inspection.save()` writes schema 2 with this
report once at the manifest top level and per-frame PNG/pose provenance below it.
