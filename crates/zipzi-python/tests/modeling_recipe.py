"""Same source-bound modeling recipe in isolated Native and Connected installs."""
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import zipzi


def uid(n):
    return f"74100000-0000-4000-8000-{n:012d}"


def recipe(session, folder, check_preview=False):
    mesh, parameter, binding, asset, blend_parameter, table, blend = map(uid, range(2, 9))
    appearance = {"opacity": 1., "multiply": [1., 1., 1.], "screen": [0., 0., 0.]}
    points = [{"x": 0., "y": 0.}, {"x": 1., "y": 0.}, {"x": 0., "y": 1.}]
    history = session.history_lengths()
    with session.edit("modeling") as edit:
        edit.create_rgba_image(asset, "generated", 1, 1, bytes([12, 34, 56, 255]))
        edit.create_mesh(zipzi.MeshRecordSpec(mesh, "Triangle", zipzi.MeshGeometryData(
            [0, 1, 2], [(0, 0), (1, 0), (0, 1)], [(0, 0), (1, 0), (0, 1)], [(0, 1, 2)]), zipzi.MeshDrawingData(asset)))
        edit.create_parameter(parameter, "Angle", 0, 1, 0)
        edit.create_parameter(blend_parameter, "Smile", 0, 1, 0, kind="blend_shape")
        edit.apply_modeling({"op": "create_binding", "binding": {
            "id": binding, "mesh_id": mesh,
            "axes": [{"id": uid(20), "parameter_id": parameter, "samples": [{"id": uid(21), "value": 0.}, {"id": uid(22), "value": 1.}]}],
            "keyforms": [{"samples": [uid(21 + n)], "positions": [{"x": p["x"] + 10 * n, "y": p["y"]} for p in points], "appearance": appearance, "draw_order": None} for n in range(2)]}},
            {"op": "create_blend_key_table", "table": {"id": table, "parameter_id": blend_parameter,
                "samples": [{"id": uid(23), "value": 0.}, {"id": uid(24), "value": 1.}], "base_sample_id": uid(23)}},
            {"op": "create_blend_binding", "binding": {"id": blend, "target_id": mesh, "target_kind": "mesh", "key_table_id": table, "constraint_ids": [],
                "keyforms": {"type": "mesh", "items": [{"sample_id": uid(23 + n), "positions": [{"x": 2 * n, "y": 0.}] * 3, "opacity": None, "draw_order": None, "multiply": None, "screen": None} for n in range(2)]}}})
        queried = edit.modeling_query(meshes=[mesh], bindings=[binding], assets=[asset])
        assert queried["records"]["assets"][0]["name"] == "generated"
        samples = [{parameter: x, blend_parameter: y} for x, y in [(0., 0.), (.5, .5), (1., 1.)]]
        sampled = edit.sample_modeling(samples)["samples"]
        assert [frame["drawables"][0]["positions"][0]["x"] for frame in sampled["frames"]] == [0., 6., 12.]
        assert session.history_lengths() == history
        assert not session.modeling_query()["records"]["ids"]["meshes"]
        with edit.plan_modeling("scene", {"operation": "patch", "id": mesh,
            "expected": {"field": "label", "value": "Triangle"}, "value": {"field": "label", "value": "Face"}}) as plan:
            assert plan.report["affected_ids"] == [mesh]
            edit.apply_modeling_plan(plan)
        assert edit.modeling_query(meshes=[mesh])["records"]["meshes"][0]["name"] == "Face"
    version, history = session.version, session.history_lengths()
    preview = None
    if check_preview:
        session.set_preview_values({parameter: .25, blend_parameter: .75})
        preview = (session.preview_values, session.preview_revision, session.preview_evaluation_count)
    sampled = session.sample_modeling(samples)["samples"]
    if preview is not None:
        assert preview == (session.preview_values, session.preview_revision, session.preview_evaluation_count)
    assert session.version == version and session.history_lengths() == history
    plan = session.plan_modeling("scene", {"operation": "patch", "id": mesh,
        "expected": {"field": "label", "value": "Face"}, "value": {"field": "label", "value": "Final"}})
    with session.edit("planned rename") as edit:
        edit.apply_modeling_plan(plan)
    plan.close()
    session.undo()
    assert session.modeling_query(meshes=[mesh])["records"]["meshes"][0]["name"] == "Face"
    session.redo()
    saved = session.save(folder / "project")
    assert saved.manifest.exists()
    # Exclude per-instance/source identities; compare domain data and effects.
    return {"positions": [f["drawables"][0]["positions"] for f in sampled["frames"]],
        "parameters": sampled["parameters"], "diagnostics": sampled["diagnostics"],
        "history": session.history_lengths(), "modified": session.modified,
        "image": session.modeling_query(assets=[asset])["records"]["assets"][0]["content"]["sha256"]}


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="zzmodel-", dir="/tmp") as temporary:
        folder = Path(temporary)
        if sys.argv[1] == "native":
            result = recipe(zipzi.Session(uid(1)), folder, check_preview=True)
        else:
            connection = zipzi.connect(host=os.environ["ZIPZI_TEST_HOST"], directory=folder)
            try:
                result = recipe(connection.create_session(uid(1)), folder)
            finally:
                connection.close()
                os.kill(connection.host_pid, signal.SIGTERM)
        print(json.dumps(result, sort_keys=True))
