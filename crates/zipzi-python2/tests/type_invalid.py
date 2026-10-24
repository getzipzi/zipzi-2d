"""The runner requires exactly these three type failures, without suppressions."""
import zipzi
from zipzi import ops

def bad(edit: zipzi.Edit) -> None:
    edit.apply({"op": "rename", "id": "x", "name": "Face"})
    edit.meshes.rename("x", 7)
    edit.apply(ops.SetPositions("x", ("bad",), ((0., 0.),)))
