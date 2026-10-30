"""Public R5/P5 authoring slice; playback/Observer acceptance remains separate."""
from contextlib import ExitStack
from dataclasses import replace
import json
from pathlib import Path

import zipzi
from zipzi import modeling as m, parameters, scene


def uid(n: int) -> str:
    return f"79600000-0000-4000-8000-{n:012x}"


def run(runtime: zipzi.Runtime, folder: Path) -> dict[str, object]:
    folder.mkdir()
    session = runtime.sessions.create(uid(1))
    parameter, part = uid(2), uid(3)
    query = m.ModelingQuery(motions=(uid(11),), motion_groups=(uid(10),),
        motion_registrations=(uid(12),), expressions=(uid(20),), physics=True, pose=True)
    before = session.info()
    with session.edit("Four animation domains") as edit:
        edit.parameters.create(parameters.ParameterSpec("P", -1., 1., 0., id=parameter, runtime_id="P"))
        edit.apply(scene.CreatePart(scene.PartSpec("Part", id=part)))
        edit.apply_many((
            m.commands.ModelingCommandMotion(intent=m.motion.MotionEditCreateGroup(id=uid(10), name="Idle")),
            m.commands.ModelingCommandMotion(intent=m.motion.MotionEditCreate(id=uid(11), name="Clip",
                registration_id=uid(12), destination=m.motion.MotionContainerGroup(value=uid(10)))),
            m.commands.ModelingCommandExpression(intent=m.expression.ExpressionEditCreate(id=uid(20), name="Smile")),
            m.commands.ModelingCommandExpression(intent=m.expression.ExpressionEditCreateEntry(owner=uid(20), id=uid(21))),
            m.commands.ModelingCommandPhysics(intent=m.physics.PhysicsEditCreateGroup(asset_id=uid(30), id=uid(31), name="Rig")),
            m.commands.ModelingCommandPose(intent=m.pose.PoseEditCreateGroup(pose_id=uid(40), id=uid(41), name="Pose")),
            m.commands.ModelingCommandPose(intent=m.pose.PoseEditCreateEntry(group_id=uid(41), id=uid(42))),
        ))
        selected = edit.modeling.records(query)
        assert selected.source.sequence == 3
        assert selected.value.motion_registrations[0].clip_id == uid(11)
        assert selected.value.motions[0].name == "Clip"
        assert selected.value.physics.id == uid(30) and selected.value.pose.id == uid(40)
        clip = replace(selected.value.motions[0], duration=1., tracks=(m.motion.MotionTrack(
            id=uid(13), target=m.motion.MotionTrackTargetParameter(parameter_id=parameter),
            initial=m.motion.MotionKey(id=uid(14), time=0., value=-1.),
            segments=(m.motion.MotionSegmentLinear(end=m.motion.MotionKey(id=uid(15), time=1., value=1.)),),
            extensions={}),))
        edit.apply(m.commands.ModelingCommandReplaceMotion(clip=clip))
        event = m.motion.MotionEvent(id=uid(16), time=.25, value="tick", extensions={})
        with edit.plan(m.commands.ModelingCommandMotion(intent=m.motion.MotionEditSetEvent(clip=uid(11), event=event))) as plan:
            assert plan.report.source == edit.modeling.records(query).source
            assert uid(11) in plan.report.affected_ids
            edit.apply(plan)
        entry = selected.value.expressions[0].entries[0]
        fields = m.expression.ExpressionEntryFields(target=m.expression.ExpressionTargetResolved(parameter_id=parameter),
            value=.75, blend=m.expression.ExpressionBlend.OVERWRITE)
        edit.apply(m.commands.ModelingCommandExpression(intent=m.expression.ExpressionEditPatchEntry(id=entry.id,
            expected=m.expression.ExpressionEntryFields(target=entry.target, value=entry.value, blend=entry.blend), fields=fields)))
        edit.apply(m.commands.ModelingCommandPhysics(intent=m.physics.PhysicsEditCreateInput(group_id=uid(31), id=uid(32))))
        edit.apply(m.commands.ModelingCommandPhysics(intent=m.physics.PhysicsEditPatch(patch=m.physics.PhysicsPatch(
            asset_id=uid(30), asset_changes=(m.physics.PhysicsAssetPropertyFps(value=30.),),
            rig=m.physics.PhysicsRigPatch(group_id=uid(31), changes=(
                m.physics.PhysicsPropertyInputParameter(id=uid(32), target=m.physics.PhysicsTargetResolved(parameter_id=parameter)),
                m.physics.PhysicsPropertyInputWeight(id=uid(32), value=75.),
            ))))))
        old_part = selected.value.pose.groups[0].entries[0].part
        edit.apply(m.commands.ModelingCommandPose(intent=m.pose.PoseEditPatch(id=uid(42),
            expected=m.pose.PoseFieldsEntry(part=old_part),
            fields=m.pose.PoseFieldsEntry(part=m.pose.PosePartRefResolved(part_id=part)))))
        candidate = edit.modeling.records(query)
        assert candidate.value.motions[0].events == (event,)
        assert candidate.value.physics.data.meta.Fps == 30.
        assert candidate.value.physics.data.settings[0].inputs[0].weight == 75.
        assert candidate.value.pose.groups[0].entries[0].part == m.pose.PosePartRefResolved(part_id=part)
        assert session.info() == before
        assert session.modeling.records(query).value.motions == (None,)
        empty = edit.modeling.records(m.ModelingQuery())
        assert empty.value.physics is None and empty.value.pose is None
        assert empty.value.ids["physics"] == (uid(30),)
        assert empty.value.ids["motion_registrations"] == (uid(12),)
    assert session.info().history == (1, 0)
    assert session.modeling.records(query).value == candidate.value
    missing = session.modeling.records(m.ModelingQuery(motions=(uid(11), uid(999), uid(11))))
    assert missing.value.motions == (candidate.value.motions[0], None, candidate.value.motions[0])

    with ExitStack() as handles:
        for kind, id, domain, expected in (
            ("motion", uid(11), m.addressing.CapabilityDomain.MOTION, m.addressing.CapabilitiesMotion),
            ("expression", uid(20), m.addressing.CapabilityDomain.EXPRESSION, m.addressing.CapabilitiesExpression),
            ("physics", uid(30), m.addressing.CapabilityDomain.PHYSICS, m.addressing.CapabilitiesPhysics),
            ("pose", uid(40), m.addressing.CapabilityDomain.POSE, m.addressing.CapabilitiesPose),
        ):
            reference = handles.enter_context(session.reference(kind, id))
            assert isinstance(reference.capabilities(domain).value, expected)
        for collection in (m.queries.CollectionQueryMotion(collection=m.motion.MotionContainerRoot()),
                m.queries.CollectionQueryMotionGroups(), m.queries.CollectionQueryClips(), m.queries.CollectionQueryExpressions(),
                m.queries.CollectionQueryExpressionEntries(expression_id=uid(20)),
                m.queries.CollectionQueryPhysics(collection=m.physics.PhysicsCollectionGroups()),
                m.queries.CollectionQueryPose(collection=m.pose.PoseCollectionGroups())):
            assert isinstance(session.capabilities(collection).value, m.addressing.CapabilitiesCollection)
        cases = (
            (m.addressing.AddressSelectorMotionRegistration(id=uid(12)),
             m.addressing.AddressPatchMotionRegistration(entry=replace(candidate.value.motion_registrations[0], fade_in=.5))),
            (m.addressing.AddressSelectorExpressionEntry(id=uid(21)),
             m.addressing.AddressPatchExpressionEntry(fields=replace(fields, value=.25))),
            (m.addressing.AddressSelectorPhysics(id=uid(32)),
             m.addressing.AddressPatchPhysics(changes=(m.physics.PhysicsPropertyInputWeight(id=uid(32), value=50.),))),
            (m.addressing.AddressSelectorPose(id=uid(41)), m.addressing.AddressPatchPose(fields=m.pose.PoseFieldsGroup(name="Changed"))),
        )
        for selector, patch in cases:
            with session.address(selector) as address, address.plan(patch) as plan:
                source = session.info()
                with session.edit("Addressed animation edit") as edit:
                    edit.apply(plan)
                session.undo().wait()
                try:
                    address.plan(patch)
                except zipzi.SdkFailure as stale:
                    assert stale.code == "STALE_FIELD_ADDRESS", stale.code
                else:
                    raise AssertionError("Undo revived an old animation field address")
                assert session.modeling.records(query).value == candidate.value
                assert session.info().history == (source.history[0], 1)
    # Child payload changes keep the root FadeIn address; changing FadeIn then
    # undoing it still retires that address. Exercise this on installed wheels too.
    root_patch = m.addressing.AddressPatchPose(fields=m.pose.PoseFieldsAsset(fade_in=.25))
    with session.address(m.addressing.AddressSelectorPose(id=uid(40))) as root:
        with session.edit("Pose child fields") as edit:
            edit.apply(m.commands.ModelingCommandPose(intent=m.pose.PoseEditPatch(id=uid(41),
                expected=m.pose.PoseFieldsGroup(name="Pose"), fields=m.pose.PoseFieldsGroup(name="Temporary"))))
        with root.plan(root_patch):
            pass
        session.undo().wait()
        with root.plan(root_patch) as plan:
            with session.edit("Pose root fields") as edit:
                edit.apply(plan)
        session.undo().wait()
        try:
            root.plan(root_patch)
        except zipzi.SdkFailure as stale:
            assert stale.code == "STALE_FIELD_ADDRESS", stale.code
        else:
            raise AssertionError("Undo revived the root Pose field address")
    assert session.modeling.records(query).value == candidate.value
    # Same generic Job path is used for asynchronous animation planning.
    with session.plan_async(m.commands.ModelingCommandMotion(intent=m.motion.MotionEditRenameGroup(id=uid(10), name="Renamed"))) as job:
        with job.wait() as plan:
            with session.edit("Background animation plan") as edit:
                edit.apply(plan)
    assert session.modeling.records(query).value.motion_groups[0].name == "Renamed"
    info = session.info()
    try:
        with session.edit("Atomic mixed-domain failure") as edit:
            edit.parameters.patch(parameter, parameters.ParameterPatch(name="Must roll back"))
            edit.apply(m.commands.ModelingCommandMotion(intent=m.motion.MotionEditRenameGroup(id=uid(999), name="Missing")))
    except zipzi.SdkFailure:
        pass
    else:
        raise AssertionError("Missing animation object accepted")
    assert session.info() == info and session.parameters.get(parameter).value.name == "P"

    # Existing sidecar exporters independently inspect the actual authored data.
    for format, name, id in ((m.project.ExportFormat.MOTION3, "clip.motion3.json", uid(11)),
            (m.project.ExportFormat.EXPRESSION3, "smile.exp3.json", uid(20)),
            (m.project.ExportFormat.PHYSICS3, "rig.physics3.json", None),
            (m.project.ExportFormat.POSE3, "pose.pose3.json", None)):
        report = session.project.export(folder / name, format, object_id=id).wait()
        assert report.published
    motion = json.loads((folder / "clip.motion3.json").read_text())
    assert motion["Curves"][0]["Segments"] == [0., -1., 0, 1., 1.]
    assert motion["UserData"] == [{"Time": .25, "Value": "tick"}]
    expression = json.loads((folder / "smile.exp3.json").read_text())
    assert expression["Parameters"] == [{"Id": "P", "Value": .75, "Blend": "Overwrite"}]
    assert session.info() == info
    saved = session.project.save(folder / "project").wait()
    session.project.open(saved.manifest).wait()
    restored = session.modeling.records(query).value
    assert restored.motions == candidate.value.motions and restored.expressions == candidate.value.expressions
    assert restored.physics == candidate.value.physics and restored.pose == candidate.value.pose
    assert session.info().history == (0, 0)
    return dict(key_values=(restored.motions[0].tracks[0].initial.value,
                           restored.motions[0].tracks[0].segments[0].end.value),
                expression_value=restored.expressions[0].entries[0].value,
                physics_weight=restored.physics.data.settings[0].inputs[0].weight,
                pose_part=restored.pose.groups[0].entries[0].part.part_id,
                event=restored.motions[0].events[0].value, formats=4)
