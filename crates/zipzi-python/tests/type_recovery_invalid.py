"""Each marked public misuse must produce exactly the named diagnostic."""
import zipzi
from zipzi import meshes


def bad(edit: zipzi.Edit, job: zipzi.Job[zipzi.Receipt], error: zipzi.OutcomeUnknown) -> None:
    zipzi.recover_call(edit.meshes.create, "spec")  # error: reportArgumentType
    zipzi.recover_call(job.wait, timeout="soon")  # error: reportArgumentType
    receipt: zipzi.Receipt = zipzi.recover_call(edit.commit)  # error: reportAssignmentType
    id: str = error.recover()  # error: reportAssignmentType
    wrong_batch: tuple[meshes.Mesh, ...] = edit.meshes.get_many(("id",))  # error: reportAssignmentType
    wrong_snapshot: meshes.Mesh = edit.meshes.get("id")  # error: reportAssignmentType
    edit.plan(meshes.Create(meshes.MeshSpec("Face", meshes.Geometry((), (), (), ()))))  # error: reportArgumentType
    print(receipt, id, wrong_batch, wrong_snapshot)
