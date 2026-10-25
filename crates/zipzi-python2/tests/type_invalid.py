"""The runner requires exactly these six type failures, without suppressions."""
import zipzi
from zipzi import ops, parameters

def bad(edit: zipzi.Edit) -> None:
    edit.apply({"op": "rename", "id": "x", "name": "Face"})
    edit.meshes.rename("x", 7)
    edit.apply(ops.SetPositions("x", ("bad",), ((0., 0.),)))
    edit.parameters.patch("x", parameters.Patch(minimum="wrong"))
    edit.parameters.get_many((7,))

def bad_cursor(runtime: zipzi.Runtime) -> None:
    runtime.events_since("cursor")
