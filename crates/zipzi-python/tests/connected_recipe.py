"""Same public Mesh/Parameter/history/error recipe for both installed backends."""
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import zipzi

D, M, P = (f"73100000-0000-4000-8000-{i:012d}" for i in range(1,4))


def recipe(session, folder):
    with session.edit("create") as edit:
        edit.create_mesh(zipzi.MeshRecordSpec(M, "Triangle", zipzi.MeshGeometryData(
            [0,1,2], [(0,0),(1,0),(0,1)], [(0,0),(1,0),(0,1)], [(0,1,2)]), zipzi.MeshDrawingData("")))
        edit.create_parameter(P, "Angle", -1,1,0)
        assert edit.mesh(M).name == "Triangle"
        assert edit.parameter(P).name == "Angle"
    stale_version = session.version
    with session.edit("modify") as edit:
        edit.rename_mesh(M, "Face")
        edit.update_positions(M, [1], [(2.5,-3.)])
        edit.replace_parameter(P, "Yaw", -2,2,.5)
    batch = session.query_objects(meshes=[M, "missing"], parameters=[P])
    assert batch.meshes[1] is None
    output = {"mesh": list(batch.meshes[0][:-1]), "parameter": list(batch.parameters[0][:-1])}
    session.undo()
    output["undone"] = session.mesh(M).name
    with session.edit("noop"):
        pass
    output["noop_history"] = session.history_lengths()
    session.redo()
    errors = []
    edit = session.edit("invalid")
    try: edit.rename_mesh("73100000-0000-4000-8000-000000000999", "missing")
    except zipzi.SdkFailure as error: errors.append([error.code,error.operation,error.object_ids])
    try: edit.commit()
    except zipzi.SdkFailure as error: errors.append([error.code,error.operation,error.object_ids])
    edit.cancel()
    output["errors"] = errors
    try: session.save(folder / "stale", expected_version=stale_version)
    except zipzi.SdkFailure as error:
        output["stale_save"] = [error.code, error.operation, error.expected_version[1:], error.actual_version[1:]]
    else: raise AssertionError("stale save published")
    assert not (folder / "stale").exists()
    output["history"] = session.history_lengths()
    saved = session.save(folder / "project")
    assert saved.manifest.is_file()
    output["modified_after_save"] = session.modified
    return output


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="zzrecipe-", dir="/tmp") as temporary:
        folder = Path(temporary)
        if sys.argv[1] == "native":
            result = recipe(zipzi.Session(D), folder)
        else:
            c = zipzi.connect(host=os.environ["ZIPZI_TEST_HOST"], directory=folder)
            try:
                result = recipe(c.create_session(D), folder)
            finally:
                c.close()
                os.kill(c.host_pid, signal.SIGTERM)
        print(json.dumps(result, sort_keys=True))
