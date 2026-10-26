"""The runner requires twenty precise diagnostics, without suppressions."""
import zipzi
from zipzi import meshes, resources
from zipzi import bindings, parameters

def bad(edit: zipzi.Edit) -> None:
    edit.apply({"op": "rename", "id": "x", "name": "Face"})  # error: reportCallIssue, reportArgumentType
    edit.meshes.rename("x", 7)  # error: reportArgumentType
    edit.apply(meshes.SetPositions("x", ("bad",), ((0., 0.),)))  # error: reportArgumentType
    edit.parameters.patch("x", parameters.ParameterPatch(minimum="wrong"))  # error: reportArgumentType
    edit.parameters.get_many((7,))  # error: reportArgumentType

def bad_cursor(runtime: zipzi.Runtime) -> None:
    runtime.events_since("cursor")  # error: reportArgumentType

def bad_modeling(edit: zipzi.Edit) -> None:
    edit.bindings.set_keyform("x", bindings.Keyform(("bad",), ((0., 0.),)))  # error: reportArgumentType
    edit.modeling.sample(({"x": "wrong"},))  # error: reportArgumentType

def bad_resources(edit: zipzi.Edit) -> None:
    edit.resources.create_image("x", b"rgba")  # error: reportArgumentType
    edit.apply("plan-token")  # error: reportCallIssue, reportArgumentType
    edit.apply(resources.CreateImage(7, "image-token"))  # error: reportArgumentType

from zipzi import scene
from zipzi import modeling

def bad_animation(session: zipzi.Session, edit: zipzi.Edit) -> None:
    edit.apply(modeling.ModelingCommandMotion(intent=modeling.PoseEditCreate(id="id")))  # error: reportArgumentType
    session.modeling.records(modeling.ModelingQuery(motions=(7,)))  # error: reportArgumentType

def bad_scene(session: zipzi.Session, edit: zipzi.Edit) -> None:
    session.reference("mesh", 7)  # error: reportArgumentType
    edit.plan({"op": "delete_scene"})  # error: reportArgumentType
    edit.apply(scene.SetOffscreenStaticKeyform("x", "zero"))  # error: reportArgumentType
    edit.apply(scene.InsertMeshVertex("x", .5, (0., 0.), (0., 0.), ()))  # error: reportArgumentType
