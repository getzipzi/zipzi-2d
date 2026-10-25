"""These public calls must type-check; this file is not executed."""
from pathlib import Path
from typing import assert_type
import zipzi
from zipzi import bindings, sampling, events, meshes, ops, parameters, resources, scene, references

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


def bound_cpu(session: zipzi.Session, face: str, angle: str) -> sampling.Batch:
    with session.edit("binding") as edit:
        id: str = edit.bindings.create(bindings.MeshBindingSpec(face, (bindings.Axis(angle, (0., 1.)),), (
            bindings.Keyform((0.,), ((0., 0.), (1., 0.), (0., 1.))),
            bindings.Keyform((1.,), ((10., 0.), (11., 0.), (10., 1.))))))
        snapshot: bindings.Snapshot = edit.bindings.get(id)
        batch: sampling.Batch = edit.modeling.sample(({angle: .5},), expected=snapshot.source)
        drawable: sampling.Drawable = batch.frames[0].drawables[0]
        opacity: float = drawable.opacity
        assert opacity == 1.
        edit.bindings.set_keyform(id, bindings.Keyform((1.,), ((20., 0.), (21., 0.), (20., 1.))))
    return session.modeling.sample(({angle: .5},), checks=sampling.Checks(.01))

def scene_and_identity(session: zipzi.Session, face: str) -> references.SceneCapabilities:
    with session.edit("organization") as edit:
        result: zipzi.WriteResult = edit.apply(scene.CreatePart(scene.PartSpec("Folder")))
        part: str = result.ids[0]
        edit.apply(scene.MoveToPart((face,), part_id=part))
        view: scene.Snapshot = edit.scene.get(face)
        assert view.value.part_id == part
        with edit.scene.plan(scene.SetSceneLabel(face, view.value.name, "Final")) as plan:
            edit.apply_plan(plan)
    with session.reference("mesh", face) as reference:
        source: zipzi.Source = reference.resolve()
        assert source.session == session.id.serial
        with reference.label() as address:
            with address.plan("Reviewed") as plan:
                with session.edit("label") as edit:
                    edit.apply_plan(plan)
        return reference.scene_capabilities()


def typed_recovery(edit: zipzi.Edit, spec: meshes.MeshSpec, id: str) -> zipzi.Receipt:
    assert_type(zipzi.recover_call(edit.meshes.create, spec), str)
    assert_type(zipzi.recover_call(edit.scene.get, id), scene.Snapshot)
    assert_type(zipzi.recover_call(edit.commit), zipzi.Job[zipzi.Receipt])
    assert_type(edit.state, zipzi.EditState)
    assert_type(edit.receipt, zipzi.Receipt | None)
    job = zipzi.recover_call(edit.commit)
    assert_type(zipzi.recover_call(job.wait, timeout=1.), zipzi.Receipt)
    return zipzi.recover_call(job.wait)
