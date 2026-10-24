"""These public calls must type-check; this file is not executed."""
from pathlib import Path
import zipzi
from zipzi import meshes, ops, resources

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
