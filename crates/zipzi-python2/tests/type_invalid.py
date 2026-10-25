"""The runner requires exactly these sixteen type failures, without suppressions."""
import zipzi
from zipzi import bindings, ops, parameters

def bad(edit: zipzi.Edit) -> None:
    edit.apply({"op": "rename", "id": "x", "name": "Face"})
    edit.meshes.rename("x", 7)
    edit.apply(ops.SetPositions("x", ("bad",), ((0., 0.),)))
    edit.parameters.patch("x", parameters.Patch(minimum="wrong"))
    edit.parameters.get_many((7,))

def bad_cursor(runtime: zipzi.Runtime) -> None:
    runtime.events_since("cursor")

def bad_modeling(edit: zipzi.Edit) -> None:
    edit.bindings.set_keyform("x", bindings.Keyform(("bad",), ((0., 0.),)))
    edit.modeling.sample(({"x": "wrong"},))

def bad_resources(edit: zipzi.Edit) -> None:
    edit.resources.create_image("x", b"rgba")
    edit.apply(ops.ApplyPlan("plan-token"))
    edit.apply(ops.CreateImage(7, "image-token"))

from zipzi import scene

def bad_scene(session: zipzi.Session, edit: zipzi.Edit) -> None:
    session.reference("mesh", 7)
    edit.scene.plan({"op": "delete_scene"})
    edit.apply(scene.SetOffscreenStaticKeyform("x", "zero"))
    edit.apply(scene.InsertMeshVertex("x", .5, (0., 0.), (0., 0.), ()))
