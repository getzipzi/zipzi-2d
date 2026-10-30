"""Public P3B scenarios, shared by source and isolated installed acceptance."""
from collections.abc import Callable
from dataclasses import replace
from math import isclose, sqrt
from pathlib import Path

import zipzi
from zipzi import bindings, meshes, modeling as m, parameters, sampling, scene

Points = tuple[tuple[float, float], ...]
Frames = tuple[Points, ...]


def uid(n: int) -> str:
    return f"79800000-0000-4000-8000-{n:012x}"


GEOMETRY = meshes.Geometry(
    (42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
    ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),),
)


def axis(n: int, parameter: str) -> m.bindings.BindingAxis:
    return m.bindings.BindingAxis(id=uid(n), parameter_id=parameter, samples=(
        m.bindings.BindingSample(id=uid(n + 1), value=0.),
        m.bindings.BindingSample(id=uid(n + 2), value=1.),
    ))


def binding(value: m.bindings.BindingAuthoringEdit) -> m.commands.ModelingCommandBinding:
    return m.commands.ModelingCommandBinding(intent=value)


def grid(value: m.bindings.MeshGridEdit) -> m.commands.ModelingCommandBinding:
    return binding(m.bindings.BindingAuthoringEditMeshGrid(value=value))


def points(result: zipzi.Snapshot[sampling.Result], mesh: str) -> Frames:
    return tuple(next(d.positions for d in f.drawables if d.id == mesh)
                 for f in result.value.frames)


def close_points(actual: Frames, expected: Frames) -> None:
    assert len(actual) == len(expected)
    for frame, wanted in zip(actual, expected, strict=True):
        assert len(frame) == len(wanted)
        for point, target in zip(frame, wanted, strict=True):
            assert all(isclose(a, b, abs_tol=1e-4) for a, b in zip(point, target, strict=True)), (point, target)


def refuses(code: str, call: Callable[[], object]) -> None:
    try:
        call()
    except zipzi.SdkFailure as error:
        assert error.code == code, (error.code, str(error))
    else:
        raise AssertionError(f"Expected {code}")


def conversions(runtime: zipzi.Runtime, destination: Path) -> dict[str, object]:
    session = runtime.sessions.create(uid(1))
    mesh, parameter, other, parent, part, owner = map(uid, (2, 3, 4, 5, 6, 7))
    with session.edit("Conversion fixture") as edit:
        edit.meshes.create(meshes.MeshSpec("Mesh", GEOMETRY, id=mesh))
        edit.parameters.create(parameters.ParameterSpec("P", 0., 1., 0., id=parameter))
        edit.parameters.create(parameters.ParameterSpec("Q", -1., 1., 0., id=other))
        edit.apply(scene.CreatePart(scene.PartSpec("Folder", id=part)))
        edit.apply(scene.CreateRotation("Affine parent", scene.Placement(), id=parent))
        transform = edit.modeling.records(m.ModelingQuery(transforms=(parent,))).value.transforms[0]
        assert isinstance(transform.data, m.scene.TransformDataRotation)
        edit.apply(m.commands.ModelingCommandReplaceTransform(transform=replace(transform,
            data=m.scene.TransformDataRotation(value=replace(transform.data.value,
                base_angle=12., pose=m.scene.RotationPose(origin=m.common.PreciseVec2(x=5., y=-3.),
                    angle=23., scale=1.75, reflect_x=True, reflect_y=False))))))
        edit.bindings.create(bindings.MeshBindingSpec(mesh, (bindings.Axis(parameter, (0., 1.)),), (
            bindings.Keyform((0.,), GEOMETRY.positions),
            bindings.Keyform((1.,), tuple((x + 10., y) for x, y in GEOMETRY.positions)),
        ), id=owner))
    poses = tuple({parameter: x} for x in (0., .25, .5, .75, 1.))
    expected = tuple(tuple((x + shift, y) for x, y in GEOMETRY.positions)
                     for shift in (0., 2.5, 5., 7.5, 10.))
    original = points(session.modeling.sample(poses), mesh)
    close_points(original, expected)
    before = session.info()
    operation = binding(m.bindings.BindingAuthoringEditReparent(value=m.scene.ReparentEdit(
        sources=(mesh,), parent_id=parent, strategy=m.scene.ReparentStrategy.PRESERVE_PARAMETER_DOMAIN,
        tolerance=.001, context=m.scene.ReparentContext(parameter_values={parameter: .5},
            parameter_domain=(m.commands.ConversionDomain(parameter_id=parameter, minimum=0., maximum=1.),),
            edit_time_seconds=0.),
    )))
    with session.reference("mesh", mesh) as reference, reference.label() as label:
        with session.plan_async(operation) as job, job.wait() as plan:
            assert session.info() == before
            assert plan.report.source.version == before.version
            assert plan.report.cost is not None and plan.report.cost.sample_combinations > 0
            assert mesh in plan.report.affected_ids
            with session.edit("Preserve domain under affine parent") as edit:
                edit.apply(plan)
                close_points(points(edit.modeling.sample(poses), mesh), expected)
                assert edit.scene.get(mesh).value.parent_id == parent
                assert session.info() == before
        reference.resolve()  # A context change preserves object identity.
        refuses("STALE_FIELD_ADDRESS", lambda: label.plan("Stale"))
        with session.edit("Organization is independent of deformation") as edit:
            edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(mesh,),
                destination=m.scene.SceneCollectionOrganization(part_id=part))))
        assert session.scene.get(mesh).value.part_id == part
        assert session.scene.get(mesh).value.parent_id == parent
        with session.edit("Expand, reorder and remap grid") as edit:
            with edit.plan(grid(m.bindings.MeshGridEditInsertSample(binding_id=owner,
                    parameter_id=parameter, value=.5))) as plan:
                edit.apply(plan)
            edit.apply(grid(m.bindings.MeshGridEditMoveSample(binding_id=owner, parameter_id=parameter,
                from_=.5, to=.4)))
            edit.apply(grid(m.bindings.MeshGridEditDeleteSample(binding_id=owner, parameter_id=parameter, value=.4)))
            p_axis = edit.bindings.get(owner).value.axes[0].id
            edit.apply(binding(m.bindings.BindingAuthoringEditGrid(value=m.bindings.BindingGridEditEdit(owner=owner,
                operation=m.bindings.GridOperationInsertSample(axis_id=p_axis, id=uid(33), value=.5)))))
            q_axis = m.bindings.BindingAxis(id=uid(30), parameter_id=other, samples=(
                m.bindings.BindingSample(id=uid(31), value=-1.), m.bindings.BindingSample(id=uid(32), value=1.)))
            edit.apply(grid(m.bindings.MeshGridEditAddAxis(binding_id=owner, axis=q_axis)))
            edit.apply(grid(m.bindings.MeshGridEditReorderAxes(binding_id=owner, parameter_ids=(other, parameter))))
            edit.apply(binding(m.bindings.BindingAuthoringEditParameterRange(value=m.parameters.ParameterRangeEdit(
                parameter_id=parameter, minimum=10., maximum=20., default_value=15.,
                policy=m.parameters.ParameterRangePolicy.MAP_KEY_VALUES))))
            record = edit.bindings.get(owner).value
            assert tuple(a.parameter_id for a in record.axes) == (other, parameter)
            assert tuple(s.value for s in record.axes[1].samples) == (10., 15., 20.)
            assert len(record.keyforms) == 6
            remapped = tuple({parameter: 10. + 10. * x, other: q}
                             for q in (-1., 0., 1.) for x in (0., .25, .5, .75, 1.))
            close_points(points(edit.modeling.sample(remapped), mesh), expected * 3)
        sample = session.bindings.get(owner).value.axes[1].samples[1].id
        with session.reference("binding_sample", sample) as sample_ref, session.address(
                m.addressing.AddressSelectorBinding(id=sample)) as address:
            with session.edit("Bake the selected slice") as edit:
                edit.apply(grid(m.bindings.MeshGridEditRemoveAxis(binding_id=owner, parameter_id=other,
                    reduction=m.bindings.AxisReductionResample(value=0.))))
                edit.apply(grid(m.bindings.MeshGridEditRemoveAxis(binding_id=owner, parameter_id=parameter,
                    reduction=m.bindings.AxisReductionResample(value=15.))))
                baked = edit.bindings.get(owner).value
                assert baked.axes == () and baked.keyforms == ()
            close_points(points(session.modeling.sample(({},)), mesh), (expected[2],))
            history = session.info().history
            assert history == (5, 0)
            session.undo().wait()
            assert len(session.bindings.get(owner).value.axes) == 2
            refuses("STALE_HANDLE", sample_ref.resolve)
            refuses("STALE_HANDLE", lambda: address.plan(
                m.addressing.AddressPatchBinding(patch=m.bindings.BindingFieldPatchSampleValue(value=16.))))
            session.redo().wait()
        saved = session.project.save(destination).wait()
        session.project.open(saved.manifest).wait()
        restored = points(session.modeling.sample(({},)), mesh)
        close_points(restored, (expected[2],))
        assert session.scene.get(mesh).value.parent_id == parent
        assert session.scene.get(mesh).value.part_id == part
    return dict(sweep_x=tuple(f[0][0] for f in original), expanded_keyforms=len(record.keyforms),
                baked_x=round(restored[0][0][0], 5), conversion_history=history)


def scene_bindings(runtime: zipzi.Runtime) -> dict[str, object]:
    session = runtime.sessions.create(uid(100))
    rotation_mesh, warp_mesh, parameter, rotation, warp, part, other_part, glue, offscreen = map(uid, range(101, 110))
    appearance = m.scene.Appearance(opacity=1., multiply=(1., 1., 1.), screen=(0., 0., 0.))
    with session.edit("All scene binding targets") as edit:
        edit.parameters.create(parameters.ParameterSpec("P", 0., 1., 0., id=parameter))
        edit.apply_many((scene.CreatePart(scene.PartSpec("Bound part", id=part)),
            scene.CreatePart(scene.PartSpec("Other", id=other_part)),
            scene.CreateRotation("Rotation", scene.Placement(part_id=part), id=rotation),
            scene.CreateWarp("Warp", scene.Placement(part_id=part), (0., 0., 1., 1.), id=warp)))
        edit.meshes.create(meshes.MeshSpec("Rotating", GEOMETRY, id=rotation_mesh))
        edit.meshes.create(meshes.MeshSpec("Warping", GEOMETRY, id=warp_mesh))
        edit.apply_many((scene.MoveToPart((rotation_mesh, warp_mesh), part_id=part),
            scene.CreateGlue("Glue", rotation_mesh, warp_mesh, id=glue),
            scene.CreateOffscreen("Composite", part, id=offscreen)))
        for mesh, parent in ((rotation_mesh, rotation), (warp_mesh, warp)):
            with edit.plan(scene.ReparentLocal((mesh,), parent_id=parent)) as plan:
                edit.apply(plan)
        records = edit.modeling.records(m.ModelingQuery(transforms=(warp,), glues=(glue,), offscreens=(offscreen,))).value
        assert isinstance(records.transforms[0].data, m.scene.TransformDataWarp)
        control_points = records.transforms[0].data.value.points
        tracks = (
            m.bindings.SceneTrackRotation(target_id=m.scene.TransformId(value=rotation), keyforms=tuple(
                m.bindings.RotationKeyform(samples=(uid(121 + i),), appearance=appearance,
                    rotation=m.scene.RotationPose(origin=m.common.PreciseVec2(x=0., y=0.), angle=90. * i,
                        scale=1., reflect_x=False, reflect_y=False)) for i in range(2))),
            m.bindings.SceneTrackWarp(target_id=m.scene.TransformId(value=warp), keyforms=tuple(
                m.bindings.WarpKeyform(samples=(uid(131 + i),), appearance=appearance,
                    positions=tuple(m.common.Vec2(x=p.x + 4. * i, y=p.y) for p in control_points)) for i in range(2))),
            m.bindings.SceneTrackPart(target_id=m.scene.PartId(value=part), keyforms=tuple(
                m.bindings.PartKeyform(samples=(uid(141 + i),), draw_order=10. * i) for i in range(2))),
        )
        for n, track in zip((120, 130, 140), tracks, strict=True):
            edit.apply(m.commands.ModelingCommandCreateSceneBinding(binding=m.bindings.SceneBinding(
                id=uid(n + 3), axes=(axis(n, parameter),), track=track)))
        edit.apply(m.commands.ModelingCommandReplaceGlue(glue=replace(records.glues[0], binding=m.bindings.GlueBinding(
            axes=(axis(150, parameter),), keyforms=(
                m.bindings.GlueKeyform(samples=(uid(151),), intensity=.1),
                m.bindings.GlueKeyform(samples=(uid(152),), intensity=.9))))))
        form = m.bindings.OffscreenKeyform(opacity=1.)
        edit.apply(m.commands.ModelingCommandReplaceOffscreen(offscreen=replace(records.offscreens[0],
            keyforms=(replace(form, opacity=.2), replace(form, opacity=.8)), part_keyform_indices=(0, 1))))
    poses = ({parameter: 0.}, {parameter: .5}, {parameter: 1.})
    original = session.modeling.sample(poses)
    close_points(points(original, rotation_mesh), (
        GEOMETRY.positions, ((0., 0.), (sqrt(.5), sqrt(.5)), (-sqrt(.5), sqrt(.5))),
        ((0., 0.), (0., 1.), (-1., 0.)),
    ))
    warp_expected = tuple(tuple((x + shift, y) for x, y in GEOMETRY.positions) for shift in (0., 2., 4.))
    close_points(points(original, warp_mesh), warp_expected)
    for frame, opacity in zip(original.value.frames, (.2, .5, .8), strict=True):
        assert isclose(frame.offscreens[0].opacity, opacity, abs_tol=1e-6)
    info = session.info()
    operation = binding(m.bindings.BindingAuthoringEditParameterRange(value=m.parameters.ParameterRangeEdit(
        parameter_id=parameter, minimum=10., maximum=20., default_value=15.,
        policy=m.parameters.ParameterRangePolicy.MAP_KEY_VALUES)))
    with session.plan(operation) as plan:
        assert session.info() == info
        assert {rotation, warp, part, offscreen} <= set(plan.report.affected_ids)
        with session.edit("Map all binding targets together") as edit:
            edit.apply(plan)
            remapped = edit.modeling.sample(tuple({parameter: p} for p in (10., 15., 20.)))
            close_points(points(remapped, rotation_mesh), points(original, rotation_mesh))
            close_points(points(remapped, warp_mesh), warp_expected)
            assert tuple(f.offscreens for f in remapped.value.frames) == tuple(f.offscreens for f in original.value.frames)
            records = edit.modeling.records(m.ModelingQuery(
                scene_bindings=tuple(uid(n) for n in (123, 133, 143)), glues=(glue,), offscreens=(offscreen,))).value
            for record in records.scene_bindings:
                assert tuple(s.value for s in record.axes[0].samples) == (10., 20.)
            assert tuple(s.value for s in records.glues[0].binding.axes[0].samples) == (10., 20.)
            assert all(isclose(f.intensity, value, abs_tol=1e-6)
                       for f, value in zip(records.glues[0].binding.keyforms, (.1, .9), strict=True))
            assert records.offscreens[0].part_keyform_indices == (0, 1)
    session.undo().wait()
    close_points(points(session.modeling.sample(poses), warp_mesh), warp_expected)
    session.redo().wait()
    with session.edit("Independent scene collections") as edit:
        edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditEnsureDrawingGroup(owner=part, parent="")))
        edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(warp_mesh, rotation_mesh),
            destination=m.scene.SceneCollectionDrawing(owner=part))))
        edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(warp_mesh,),
            destination=m.scene.SceneCollectionDrawing(owner=part), before=rotation_mesh)))
        drawing = edit.modeling.records(m.ModelingQuery()).value.draw_order_groups
        edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(warp_mesh,),
            destination=m.scene.SceneCollectionOrganization(part_id=other_part))))
        assert edit.scene.get(warp_mesh).value.parent_id == warp
        with edit.plan(scene.ReparentLocal((warp_mesh,), parent_id=rotation)) as plan:
            edit.apply(plan)
        edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(warp_mesh,),
            destination=m.scene.SceneCollectionDeformation(parent_id=rotation), before=rotation_mesh)))
        assert edit.scene.get(warp_mesh).value.part_id == other_part
        assert edit.scene.get(warp_mesh).value.parent_id == rotation
        after_drawing = edit.modeling.records(m.ModelingQuery()).value.draw_order_groups
        # SDK plans may normalize drawing ranges; membership is independent.
        assert {g.owner: g.items for g in after_drawing} == {g.owner: g.items for g in drawing}
        assert next(g.items for g in drawing if g.owner == part) == (warp_mesh, rotation_mesh)
    info = session.info()
    invalid_move = m.commands.ModelingCommandScene(intent=m.scene.SceneEditMove(ids=(warp_mesh,),
        destination=m.scene.SceneCollectionDeformation(parent_id=rotation), before=uid(999)))
    refuses("INVALID_PLACEMENT", lambda: session.plan(invalid_move))
    assert session.info() == info
    return dict(scene_tracks=tuple(type(t).__name__ for t in tracks),
                warp_x=tuple(f[0][0] for f in points(original, warp_mesh)),
                offscreen_opacity=tuple(round(f.offscreens[0].opacity, 5) for f in original.value.frames),
                drawing_members=next(g.items for g in after_drawing if g.owner == part),
                independent_collections=3)


def run(runtime: zipzi.Runtime, destination: Path) -> dict[str, object]:
    return dict(conversions=conversions(runtime, destination), scene_bindings=scene_bindings(runtime))
