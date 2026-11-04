"""Build tiny, self-authored Observe inspection projects with the public SDK.

Run from the repository root with `./tools/dev-python
fixtures/observe_inspection/build_fixtures.py`. The script creates PNGs
with the Python standard library and writes three authoring projects. It
refuses to overwrite existing projects; pass --replace to regenerate them.
"""

from __future__ import annotations

import argparse
from dataclasses import replace
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import shutil
import struct
import zlib

import zipzi
from zipzi import bindings, meshes, parameters, scene, modeling as m

ROOT = Path(__file__).resolve().parent
TEXTURES = ROOT / "textures"
PROJECTS = ROOT / "projects"
UUID = "10000000-0000-4000-8000-"


def ident(number: int) -> str:
    return UUID + f"{number:012x}"


def png_rgba(width: int, height: int, pixels: bytes) -> bytes:
    assert len(pixels) == width * height * 4

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + kind + data +
                struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff))

    scanlines = b"".join(
        b"\0" + pixels[y * width * 4:(y + 1) * width * 4]
        for y in range(height)
    )
    return (b"\x89PNG\r\n\x1a\n" +
            chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(scanlines, level=9)) +
            chunk(b"IEND", b""))


def make_textures() -> dict[str, Path]:
    TEXTURES.mkdir(parents=True, exist_ok=True)
    rgba = bytearray()
    for y in range(16):
        for x in range(16):
            inside = 4 <= x < 12 and 4 <= y < 12
            rgba.extend((230, 30, 80, 255 if inside else 0))
    padding = TEXTURES / "transparent-padding.png"
    padding.write_bytes(png_rgba(16, 16, rgba))

    rgba = bytearray()
    for y in range(32):
        for x in range(32):
            rgba.extend((20, 110, 250, 255 if x == y else 0))
    line = TEXTURES / "thin-line.png"
    line.write_bytes(png_rgba(32, 32, rgba))

    rgba = bytearray()
    for y in range(32):
        for x in range(32):
            inside = (x - 15.5) ** 2 + (y - 15.5) ** 2 < 10 ** 2
            rgba.extend((255, 255, 255, 255 if inside else 0))
    mask = TEXTURES / "round-mask.png"
    mask.write_bytes(png_rgba(32, 32, rgba))
    return {"padding": padding, "line": line, "mask": mask}


def new_session(runtime: zipzi.Runtime, number: int) -> zipzi.Session:
    model = runtime.sessions.create(ident(number))
    setup = model.modeling.records(m.ModelingQuery()).value.setup
    with model.edit("source canvas") as edit:
        edit.apply(m.commands.ModelingCommandReplaceModelSetup(setup=replace(
            setup, source_canvas=m.scene.Canvas(width=100., height=100.,
                origin=m.common.Vec2(37., 62.), pixels_per_unit=7.5))))
    return model


def save(model: zipzi.Session, name: str) -> Path:
    PROJECTS.mkdir(parents=True, exist_ok=True)
    destination = PROJECTS / f"{name}.zipzi"
    receipt = model.project.save(destination).wait()
    assert receipt.path == destination
    assert not model.project.diagnose().wait().diagnostics
    model.close()
    return destination


def rectangle(edit: zipzi.Edit, number: int, name: str, asset: int,
              low: tuple[float, float], high: tuple[float, float]) -> None:
    x0, y0 = low
    x1, y1 = high
    geometry = meshes.Geometry((0, 1, 2, 3),
        ((x0, y0), (x1, y0), (x1, y1), (x0, y1)),
        ((0., 0.), (1., 0.), (1., 1.), (0., 1.)), ((0, 1, 2), (0, 2, 3)))
    edit.meshes.create(meshes.MeshSpec(name, geometry, id=ident(number),
                                      texture_asset_id=ident(asset)))


def images(runtime: zipzi.Runtime, stack: ExitStack, textures: dict[str, Path]):
    return {name: stack.enter_context(runtime.resources.load_png(path).wait())
            for name, path in textures.items()}


def pixel_project(runtime: zipzi.Runtime, textures: dict[str, Path]) -> Path:
    model = new_session(runtime, 1)
    with ExitStack() as stack:
        image = images(runtime, stack, textures)
        with model.edit("pixel and binding controls") as edit:
            edit.resources.create_image("padding", image["padding"], id=ident(11))
            edit.resources.create_image("one pixel line", image["line"], id=ident(12))
            rectangle(edit, 21, "same name", 11, (20., 20.), (60., 60.))
            rectangle(edit, 22, "same name", 12, (62., 24.), (82., 44.))
            edit.parameters.create(parameters.ParameterSpec("Shift", 0., 1., 0., id=ident(31)))
            edit.bindings.create(bindings.MeshBindingSpec(ident(21),
                (bindings.Axis(ident(31), (0., 1.)),), (
                    bindings.Keyform((0.,), ((20., 20.), (60., 20.), (60., 60.), (20., 60.))),
                    bindings.Keyform((1.,), ((24., 20.), (64., 20.), (64., 60.), (24., 60.))),
                ), id=ident(41)))
    return save(model, "pixel-binding")


def hierarchy_project(runtime: zipzi.Runtime, textures: dict[str, Path]) -> Path:
    model = new_session(runtime, 2)
    left, right = (20 - 37) / 7.5, (60 - 37) / 7.5
    bottom, top = (62 - 60) / 7.5, (62 - 20) / 7.5
    with runtime.resources.load_png(textures["line"]).wait() as image:
        with model.edit("rotation and compressed warp") as edit:
            edit.resources.create_image("line", image, id=ident(13))
            rectangle(edit, 23, "warped mesh", 13, (0., 0.), (1., 1.))
            edit.apply(scene.CreatePart(scene.PartSpec("head", id=ident(51))))
            edit.apply(scene.CreateRotation("rotate", scene.Placement(part_id=ident(51)), id=ident(61)))
            transform = edit.modeling.records(m.ModelingQuery(transforms=(ident(61),))).value.transforms[0]
            rotation = replace(transform.data.value,
                               pose=replace(transform.data.value.pose, origin=m.common.PreciseVec2(37., 62.), angle=25.))
            edit.apply(m.commands.ModelingCommandReplaceTransform(
                transform=replace(transform, data=m.scene.TransformDataRotation(value=rotation))))
            edit.apply(scene.CreateWarp("compressed warp", scene.Placement(
                part_id=ident(51), parent_id=ident(61)), (left, bottom, right, top), id=ident(62)))
            transform = edit.modeling.records(m.ModelingQuery(transforms=(ident(62),))).value.transforms[0]
            warp = m.scene.WarpTransform(rows=1, columns=1, quad=True,
                points=tuple(m.common.Vec2(x, y) for x, y in ((left, top), (right, top), (left + .2, top * .45), (right - .2, top * .45))))
            edit.apply(m.commands.ModelingCommandReplaceTransform(
                transform=replace(transform, data=m.scene.TransformDataWarp(value=warp))))
            edit.apply(scene.ReparentLocal((ident(23),), parent_id=ident(62)))
            edit.apply(scene.MoveToPart((ident(23),), part_id=ident(51)))
    return save(model, "rotated-warp")


def composition_project(runtime: zipzi.Runtime, textures: dict[str, Path]) -> Path:
    model = new_session(runtime, 3)
    with ExitStack() as stack:
        image = images(runtime, stack, textures)
        with model.edit("overlap, mask, and offscreen") as edit:
            edit.resources.create_image("padding", image["padding"], id=ident(14))
            edit.resources.create_image("mask", image["mask"], id=ident(15))
            edit.apply(scene.CreatePart(scene.PartSpec("offscreen group", id=ident(52))))
            for number, name, asset, low, high in (
                (24, "rear", 14, (20., 20.), (80., 80.)),
                (25, "mask source", 15, (25., 25.), (75., 75.)),
                (26, "masked target", 14, (30., 30.), (70., 70.)),
                (27, "front cover", 14, (40., 40.), (65., 65.)),
            ):
                rectangle(edit, number, name, asset, low, high)
            edit.apply(scene.MoveToPart((ident(26),), part_id=ident(52)))
            edit.apply(scene.CreateOffscreen("half-opacity layer", ident(52), id=ident(71)))
            edit.apply(scene.InsertOffscreenKeyform(ident(71), scene.OffscreenForm(.5)))
            edit.apply(m.commands.ModelingCommandScene(intent=m.scene.SceneEditPatch(
                id=ident(26), expected=m.scene.SceneFieldValueMasks(value=()),
                value=m.scene.SceneFieldValueMasks(value=(ident(25),)))))
    return save(model, "masked-offscreen")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--replace", action="store_true")
    parser.add_argument("--verify", action="store_true")
    options = parser.parse_args()
    if options.verify:
        if options.replace:
            parser.error("--verify and --replace are mutually exclusive")
        manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
        for relative, expected in manifest["files"].items():
            path = (ROOT / relative).resolve()
            if not path.is_relative_to(ROOT) or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                raise RuntimeError(f"Fixture checksum mismatch: {relative}")
        for relative in manifest["projects"]:
            with zipzi.local() as runtime:
                model = runtime.sessions.open((ROOT / relative).resolve()).wait()
                if model.project.diagnose().wait().diagnostics:
                    raise RuntimeError(f"Fixture structure invalid: {relative}")
        print(f"Verified {len(manifest['files'])} files and {len(manifest['projects'])} projects")
        return
    if PROJECTS.exists():
        if not options.replace:
            parser.error("projects already exist; pass --replace to regenerate")
        shutil.rmtree(PROJECTS)
    textures = make_textures()
    with zipzi.local() as runtime:
        projects = [pixel_project(runtime, textures), hierarchy_project(runtime, textures),
                    composition_project(runtime, textures)]
    (PROJECTS / ".gitignore").write_text(".*.zipzi.lock\n")
    files = sorted([*textures.values(), *projects])
    manifest = {
        "schema_version": 1,
        "license": "CC0-1.0; generated locally by build_fixtures.py",
        "projects": [str(path.relative_to(ROOT)) for path in projects],
        "files": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in files if path.is_file()
        },
    }
    (ROOT / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
