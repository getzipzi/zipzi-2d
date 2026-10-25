"""These public calls must type-check; this file is not executed."""
from pathlib import Path
import zipzi
from zipzi import events, meshes, ops, parameters, resources

def recipe(runtime: zipzi.Runtime) -> zipzi.SaveReceipt:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000001")
    geometry = meshes.Geometry((0, 1, 2), ((0., 0.), (1., 0.), (0., 1.)),
        ((0., 0.), (1., 0.), (0., 1.)), ((0, 1, 2),))
    with session.edit("face") as edit:
        face: str = edit.meshes.create(meshes.MeshSpec("Face", geometry))
        snapshot: meshes.Snapshot = edit.meshes.get(face)
        result: zipzi.WriteResult = edit.apply(ops.Rename(face, "Final"))
        assert result.source.version == snapshot.source.version
    return session.project.save(Path("/tmp/zipzi-python2-typing")).wait()

def textured_recipe(runtime: zipzi.Runtime) -> resources.Usage:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000002")
    with runtime.resources.prepare_image(1, 1, b"rgba") as image:
        pixels: resources.ImagePixels = image.pixels()
        with session.edit("image") as edit:
            id: str = edit.resources.create_image("Draft", image)
            snapshot: resources.Snapshot = edit.resources.get(id)
            with edit.resources.plan(resources.RenameIntent(id, "Draft", "Final")) as plan:
                report: zipzi.plans.Report = plan.report
                result: zipzi.WriteResult = edit.apply_plan(plan)
                assert result.source.sequence > report.source.sequence
            assert snapshot.pixels == pixels
            edit.resources.replace_image(id, image)
    return runtime.resources.usage()

def cancellable(job: zipzi.Job[zipzi.Receipt]) -> zipzi.Receipt:
    state: str = job.state()
    if state == "queued":
        job.cancel()
    return job.wait()

def resident(runtime: zipzi.Runtime) -> zipzi.SessionId:
    info: zipzi.SessionInfo = runtime.sessions.list()[0]
    session: zipzi.Session = runtime.sessions.attach(info.id)
    metadata: zipzi.ConnectionInfo | None = runtime.connection
    if metadata is not None and "show_window" in metadata.capabilities:
        session.show_window()
    return session.id

def desktop(host: Path) -> zipzi.Runtime:
    return zipzi.connect(start=True, host=host, startup_timeout=15., sdk_build="explicit build identity")

def parameter_and_events(runtime: zipzi.Runtime, session: zipzi.Session) -> parameters.Batch:
    stream: events.SessionSubscription = session.subscribe()
    cursor: events.Cursor = stream.cursor
    with session.edit("typed parameter") as edit:
        id = edit.parameters.create(parameters.ParameterSpec("Angle", -30., 30., 0.))
        edit.parameters.patch(id, parameters.Patch(name="Final"))
        batch: parameters.Batch = edit.parameters.get_many((id,))
        snapshots: tuple[meshes.Snapshot, ...] = edit.meshes.get_many(())
        assert snapshots == ()
    for event in stream.poll():
        if event.kind == "published":
            receipt: zipzi.Receipt = event.receipt
            assert receipt.before == batch.source.version
    replay: tuple[events.Event, ...] = runtime.events_since(cursor)
    assert len(replay) >= 1
    return batch
