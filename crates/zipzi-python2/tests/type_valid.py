"""These public calls must type-check; this file is not executed."""
from pathlib import Path
from typing import assert_type
import zipzi
from zipzi import bindings, sampling, events, meshes, modeling, ops, parameters, resources, scene, references

def recipe(runtime: zipzi.Runtime) -> zipzi.SaveReceipt:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000001")
    geometry = meshes.Geometry((0, 1, 2), ((0., 0.), (1., 0.), (0., 1.)),
        ((0., 0.), (1., 0.), (0., 1.)), ((0, 1, 2),))
    with session.edit("face") as edit:
        face: str = edit.meshes.create(meshes.MeshSpec("Face", geometry))
        snapshot: zipzi.Snapshot[meshes.Mesh] = edit.meshes.get(face)
        result: zipzi.WriteResult = edit.apply(meshes.Rename(face, "Final"))
        assert result.source.version == snapshot.source.version
    return session.project.save(Path("/tmp/zipzi-python2-typing")).wait()

def textured_recipe(runtime: zipzi.Runtime) -> resources.Usage:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000002")
    with runtime.resources.prepare_image(1, 1, b"rgba") as image:
        pixels: resources.ImagePixels = image.pixels()
        with session.edit("image") as edit:
            id: str = edit.resources.create_image("Draft", image)
            snapshot: zipzi.Snapshot[resources.Image] = edit.resources.get(id)
            with edit.plan(resources.RenameImage(id, "Final", expected_name="Draft")) as plan:
                report: zipzi.PlanReport = plan.report
                result: zipzi.WriteResult = edit.apply(plan)
                assert result.source.sequence > report.source.sequence
            assert snapshot.value.pixels == pixels
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

def parameter_and_events(runtime: zipzi.Runtime, session: zipzi.Session) -> zipzi.Batch[parameters.Parameter]:
    stream: events.SessionSubscription = session.subscribe()
    cursor: events.Cursor = stream.cursor
    with session.edit("typed parameter") as edit:
        id = edit.parameters.create(parameters.ParameterSpec("Angle", -30., 30., 0.))
        edit.parameters.patch(id, parameters.ParameterPatch(name="Final"))
        batch: zipzi.Batch[parameters.Parameter] = edit.parameters.get_many((id,))
        snapshots: zipzi.Batch[meshes.Mesh] = edit.meshes.get_many(())
        assert snapshots.items == ()
    for event in stream.poll():
        if event.kind == "published":
            receipt: zipzi.Receipt = event.receipt
            assert receipt.before == batch.source.version
    replay: tuple[events.Event, ...] = runtime.events_since(cursor)
    assert len(replay) >= 1
    return batch


def bound_cpu(session: zipzi.Session, face: str, angle: str) -> zipzi.Snapshot[sampling.Result]:
    with session.edit("binding") as edit:
        id: str = edit.bindings.create(bindings.MeshBindingSpec(face, (bindings.Axis(angle, (0., 1.)),), (
            bindings.Keyform((0.,), ((0., 0.), (1., 0.), (0., 1.))),
            bindings.Keyform((1.,), ((10., 0.), (11., 0.), (10., 1.))))))
        snapshot: zipzi.Snapshot[bindings.MeshBinding] = edit.bindings.get(id)
        batch: zipzi.Snapshot[sampling.Result] = edit.modeling.sample(({angle: .5},), expected=snapshot.source)
        drawable: sampling.Drawable = batch.value.frames[0].drawables[0]
        opacity: float = drawable.opacity
        assert opacity == 1.
        edit.bindings.set_keyform(id, bindings.Keyform((1.,), ((20., 0.), (21., 0.), (20., 1.))))
    return session.modeling.sample(({angle: .5},), checks=sampling.Checks(.01))

def scene_and_identity(session: zipzi.Session, face: str) -> references.SceneCapabilities:
    with session.edit("organization") as edit:
        result: zipzi.WriteResult = edit.apply(scene.CreatePart(scene.PartSpec("Folder")))
        part: str = result.ids[0]
        edit.apply(scene.MoveToPart((face,), part_id=part))
        view: zipzi.Snapshot[scene.Summary] = edit.scene.get(face)
        assert view.value.part_id == part
        with edit.plan(scene.SetSceneLabel(face, view.value.name, "Final")) as plan:
            edit.apply(plan)
    with session.reference("mesh", face) as reference:
        source: zipzi.Source = reference.resolve()
        assert source.session == session.id.serial
        with reference.label() as address:
            with address.plan("Reviewed") as plan:
                with session.edit("label") as edit:
                    edit.apply(plan)
        return reference.scene_capabilities()


def typed_recovery(edit: zipzi.Edit, spec: meshes.MeshSpec, id: str) -> zipzi.Receipt:
    assert_type(zipzi.recover_call(edit.meshes.create, spec), str)
    assert_type(zipzi.recover_call(edit.scene.get, id), zipzi.Snapshot[scene.Summary])
    assert_type(zipzi.recover_call(edit.commit), zipzi.Job[zipzi.Receipt])
    assert_type(edit.state, zipzi.EditState)
    assert_type(edit.receipt, zipzi.Receipt | None)
    job = zipzi.recover_call(edit.commit)
    assert_type(zipzi.recover_call(job.wait, timeout=1.), zipzi.Receipt)
    return zipzi.recover_call(job.wait)


def uniform_results(session: zipzi.Session, id: str) -> zipzi.Batch[meshes.Mesh]:
    assert_type(session.meshes.get(id), zipzi.Snapshot[meshes.Mesh])
    assert_type(session.meshes.get_many((id,)), zipzi.Batch[meshes.Mesh])
    assert_type(session.parameters.get(id), zipzi.Snapshot[parameters.Parameter])
    assert_type(session.parameters.get_many((id,)), zipzi.Batch[parameters.Parameter])
    assert_type(session.bindings.get(id), zipzi.Snapshot[bindings.MeshBinding])
    assert_type(session.resources.get(id), zipzi.Snapshot[resources.Image])
    assert_type(session.scene.get(id), zipzi.Snapshot[scene.Summary])
    assert_type(session.modeling.sample(({},)), zipzi.Snapshot[sampling.Result])
    assert_type(zipzi.recover_call(session.meshes.get_many, (id,)), zipzi.Batch[meshes.Mesh])
    return session.meshes.get_many((id,))


def canonical_conversion(session: zipzi.Session, owner: str, parameter: str) -> zipzi.PlanReport:
    operation = modeling.ModelingCommandBinding(intent=modeling.BindingAuthoringEditMeshGrid(
        value=modeling.MeshGridEditMoveSample(binding_id=owner, parameter_id=parameter, from_=.5, to=.4)))
    assert_type(session.modeling.records(modeling.ModelingQuery(bindings=(owner,))),
                zipzi.Snapshot[modeling.OwnedModelingRecords])
    assert_type(zipzi.recover_call(session.plan_async, operation), zipzi.Job[zipzi.Plan])
    with session.plan_async(operation) as job, job.wait() as plan:
        assert_type(plan, zipzi.Plan)
        with session.edit("Typed conversion") as edit:
            assert_type(edit.apply(plan), zipzi.WriteResult)
        return plan.report


def canonical_address(session: zipzi.Session, sample: str) -> zipzi.PlanReport:
    with session.address(modeling.AddressSelectorBinding(id=sample)) as address:
        assert_type(address, references.FieldAddress)
        with address.plan(modeling.AddressPatchBinding(
                patch=modeling.BindingFieldPatchSampleValue(value=.25))) as plan:
            assert_type(plan, zipzi.Plan)
            return plan.report


def unified_planning(session: zipzi.Session, edit: zipzi.Edit, id: str) -> zipzi.PlanReport:
    operation: ops.PlannableOperation = meshes.Rename(id, "Final", expected_name="Before")
    plan: zipzi.Plan = session.plan(operation)
    patch: ops.Operation = parameters.Patch(id, parameters.ParameterPatch(name="Angle"))
    assert_type(edit.apply(plan), zipzi.WriteResult)
    assert_type(edit.apply_many((plan, patch)), zipzi.WriteResult)
    assert_type(edit.plan(resources.RenameImage(id, "Texture")), zipzi.Plan)
    assert_type(zipzi.recover_call(session.plan, operation), zipzi.Plan)
    assert_type(zipzi.recover_call(edit.apply, meshes.Rename(id, "Again")), zipzi.WriteResult)
    return plan.report


def resource_project_jobs(runtime: zipzi.Runtime, session: zipzi.Session, path: Path,
                          png: bytes) -> modeling.ProjectReport:
    with runtime.resources.prepare_buffer(png) as buffer:
        assert_type(buffer, zipzi.Buffer)
        assert_type(buffer.read(0, 8), bytes)
        with runtime.resources.prepare_png(buffer) as job:
            assert_type(job, zipzi.Job[resources.PreparedImage])
            with job.wait() as image:
                assert_type(image.pixels(), resources.ImagePixels)
    assert_type(session.project.save(path), zipzi.Job[zipzi.SaveReceipt])
    assert_type(session.project.open(path), zipzi.Job[modeling.ProjectReport])
    assert_type(session.project.replace(path, modeling.ImportFormat.MODEL3), zipzi.Job[modeling.ProjectReport])
    assert_type(session.project.diagnose(), zipzi.Job[modeling.ProjectReport])
    export = zipzi.recover_call(session.project.export, path, modeling.ExportFormat.PACKAGE)
    assert_type(export, zipzi.Job[modeling.ProjectReport])
    return zipzi.recover_call(export.wait)


def animation_authoring(session: zipzi.Session, edit: zipzi.Edit, id: str) -> zipzi.Snapshot[modeling.OwnedModelingRecords]:
    operation = modeling.ModelingCommandMotion(intent=modeling.MotionEditCreateGroup(id=id, name="Motion group"))
    assert_type(edit.apply(operation), zipzi.WriteResult)
    assert_type(session.plan(operation), zipzi.Plan)
    assert_type(zipzi.recover_call(edit.plan_async, operation), zipzi.Job[zipzi.Plan])
    query = modeling.ModelingQuery(motions=(id,), expressions=(id,), physics=True, pose=True)
    snapshot = session.modeling.records(query)
    assert_type(snapshot.value.physics, modeling.PhysicsAsset | None)
    with session.address(modeling.AddressSelectorPose(id=id)) as address:
        assert_type(address.plan(modeling.AddressPatchPose(fields=modeling.PoseFieldsGroup(name="New"))), zipzi.Plan)
    return snapshot

from zipzi import animation
from zipzi import spatial

def spatial_queries(session: zipzi.Session, edit: zipzi.Edit, id: str) -> zipzi.Snapshot[spatial.Batch]:
    query=spatial.Bounds(targets=(spatial.MeshTarget(value=id),))
    assert_type(session.modeling.spatial(({},),(query,)),zipzi.Job[zipzi.Snapshot[spatial.Batch]])
    assert_type(edit.modeling.spatial(({},),(query,)),zipzi.Job[zipzi.Snapshot[spatial.Batch]])
    with session.preview() as preview:
        result=preview.seek(.5,geometry=False,queries=(spatial.Hits(point=(0.,0.)),)).wait()
        assert_type(result.value.frames[0].spatial,tuple[spatial.Result,...])
    return zipzi.recover_call(session.modeling.spatial,({},),(query,)).wait()

def animation_preview(session: zipzi.Session, edit: zipzi.Edit) -> zipzi.Snapshot[animation.Batch]:
    with session.preview() as preview:
        assert_type(preview.source,zipzi.Source)
        assert_type(preview.id,int)
        assert_type(preview.apply(animation.SetParameter(id="parameter",value=.5)),zipzi.Snapshot[animation.State])
        assert_type(zipzi.recover_call(preview.seek,.5),zipzi.Job[zipzi.Snapshot[animation.Batch]])
        assert_type(preview.advance_to(.5),zipzi.Job[zipzi.Snapshot[animation.Batch]])
        return preview.sample((0.,.5),geometry=True).wait()
