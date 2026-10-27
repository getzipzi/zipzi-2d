"""The runner requires twenty-two precise diagnostics, without suppressions."""
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

from zipzi import animation

def bad_preview(preview: animation.Preview) -> None:
    preview.apply({"kind":"reset"})  # error: reportArgumentType
    preview.seek("time")  # error: reportArgumentType

from zipzi import spatial
from zipzi import observation,inspection

def bad_observation(session:zipzi.Session,preview:animation.Preview)->None:
    session.modeling.observe(({},),{"width":64})  # error: reportArgumentType
    preview.seek(.5,view=observation.View(width="64",height=32,roi=(0.,0.,2.,1.)))  # error: reportArgumentType
    inspection.save("result","path")  # error: reportArgumentType, reportArgumentType

def bad_spatial(session: zipzi.Session, preview: animation.Preview) -> None:
    session.modeling.spatial(({},),({"kind":"hits","point":(0.,0.)},))  # error: reportArgumentType
    preview.seek(.5,queries=(spatial.Hits(point=(0.,"y")),))  # error: reportArgumentType
