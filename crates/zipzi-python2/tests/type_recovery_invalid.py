"""Each marked public misuse must produce exactly the named diagnostic."""
import zipzi


def bad(edit: zipzi.Edit, job: zipzi.Job[zipzi.Receipt], error: zipzi.OutcomeUnknown) -> None:
    zipzi.recover_call(edit.meshes.create, "spec")  # error: reportArgumentType
    zipzi.recover_call(job.wait, timeout="soon")  # error: reportArgumentType
    receipt: zipzi.Receipt = zipzi.recover_call(edit.commit)  # error: reportAssignmentType
    id: str = error.recover()  # error: reportAssignmentType
    print(receipt, id)
