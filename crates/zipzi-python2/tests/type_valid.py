"""These public calls must type-check; this file is not executed."""
from pathlib import Path
import zipzi
from zipzi import meshes, ops

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
