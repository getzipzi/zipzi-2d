"""Public resource/project workflow for both source and installed clients."""
from collections.abc import Callable
from contextlib import ExitStack
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path

import zipzi
from zipzi import meshes, modeling as m, resources


def uid(n: int) -> str:
    return f"79700000-0000-4000-8000-{n:012x}"


def attachment(edit: zipzi.Edit, intent: m.AttachmentEdit) -> zipzi.WriteResult:
    return edit.apply(m.ModelingCommandAttachment(intent=intent))


def rejected(call: Callable[[], object], code: str | None = None) -> zipzi.SdkFailure:
    try:
        call()
    except zipzi.SdkFailure as error:
        assert not isinstance(error, zipzi.OutcomeUnknown), error
        assert code is None or error.code == code, (error.code, code)
        return error
    raise AssertionError("Expected a confirmed SDK failure")


def run(runtime: zipzi.Runtime, folder: Path) -> dict[str, object]:
    folder.mkdir()
    baseline = runtime.resources.usage()
    session = runtime.sessions.create(uid(1))
    asset, mesh, data_id, empty_id, absent_id = map(uid, (2, 3, 4, 5, 6))
    width, height = 1024, 513
    pixels_a = bytes((12, 34, 56, 255)) * (width * height)
    pixels_b = bytes((65, 43, 21, 255)) * (width * height)
    geometry = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
        ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))
    with runtime.resources.prepare_buffer(pixels_a) as buffer:
        assert buffer.read((1 << 20) - 3, 17) == pixels_a[(1 << 20) - 3:(1 << 20) + 14]
        with runtime.resources.prepare_image(width, height, buffer) as image:
            with session.edit("Memory image and package attachments") as edit:
                edit.resources.create_image("Texture", image, id=asset)
                edit.meshes.create(meshes.MeshSpec("Face", geometry, texture_asset_id=asset, id=mesh))
                runtime_id = edit.scene.get(mesh).value.runtime_id
                content = json.dumps({"Version": 3, "UserData": [
                    {"Target": "ArtMesh", "Id": runtime_id, "Value": "p4-data-" * 200_000},
                ]}, separators=(",", ":")).encode()
                assert len(content) > 1 << 20
                for id, path in ((data_id, "extras/data.userdata3.json"),
                                 (empty_id, "extras/empty.bin"), (absent_id, "extras/unavailable.bin")):
                    attachment(edit, m.AttachmentEditCreate(id=id, path=path))
                selected = edit.modeling.records(m.ModelingQuery(attachments=(data_id, empty_id, absent_id)))
                assert selected.value.attachments[1].bytes is None
                attachment(edit, m.AttachmentEditReplace(expected=selected.value.attachments[0],
                    attachment=replace(selected.value.attachments[0], bytes=content)))
                attachment(edit, m.AttachmentEditReplace(expected=selected.value.attachments[1],
                    attachment=replace(selected.value.attachments[1], bytes=b"")))
                attachment(edit, m.AttachmentEditSetUserData(reference=m.AttachmentRefResolved(attachment_id=data_id)))
                owned = edit.modeling.records(m.ModelingQuery(attachments=(data_id, empty_id, absent_id)))
                assert tuple(a.bytes for a in owned.value.attachments) == (content, b"", None)
                assert owned.source.edit is not None
                assert edit.resources.get(asset).value.pixels.rgba == pixels_a
    assert runtime.resources.usage().prepared_images == 0
    assert session.info().history == (1, 0)
    first = session.project.save(folder / "original").wait()
    # Native Windows publication cannot sync the parent directory; a successful
    # save must report that limitation instead of claiming crash durability.
    assert first.durable == (os.name != 'nt') and first.manifest.is_file()
    saved_history = session.info().history

    # Project storage preserves unavailable and empty files as different values.
    opened = session.project.open(first.manifest).wait()
    assert opened.after.generation > opened.before.generation
    selected = session.modeling.records(m.ModelingQuery(attachments=(data_id, empty_id, absent_id)))
    assert tuple(a.bytes for a in selected.value.attachments) == (content, b"", None)
    assert owned.value.attachments[0].bytes == content
    assert session.resources.get(asset).value.pixels.rgba == pixels_a
    before = session.info()
    rejected(lambda: session.project.export(folder / "refused-package", m.ExportFormat.PACKAGE).wait(),
             "UNREFERENCED_ATTACHMENT")
    assert session.info() == before and not (folder / "refused-package").exists()
    with session.edit("Keep the registered attachment") as edit:
        attachment(edit, m.AttachmentEditDelete(id=empty_id))
        attachment(edit, m.AttachmentEditDelete(id=absent_id))

    # Save As copies current content and rebases old disk history to its old root.
    with ExitStack() as leases:
        original = leases.enter_context(session.resources.snapshot(asset))
        with runtime.resources.prepare_image(width, height, pixels_b) as image:
            with session.edit("Replace memory texture") as edit:
                edit.resources.replace_image(asset, image)
        history = session.info().history
        saved = session.project.save(folder / "relocated").wait()
        assert saved.durable == (os.name != 'nt') and saved.manifest.is_file()
        assert session.info().history == history
        assert session.info().modified is False
        parked = folder / "parked-original"
        first.manifest.parent.rename(parked)
        try:
            assert session.resources.get(asset).value.pixels.rgba == pixels_b
            session.undo().wait()
            rejected(lambda: session.resources.get(asset))
            assert original.pixels().rgba == pixels_a
            session.redo().wait()
            assert session.resources.get(asset).value.pixels.rgba == pixels_b
        finally:
            parked.rename(first.manifest.parent)
        session.undo().wait()
        historical = session.modeling.records(m.ModelingQuery(assets=(asset,))).value.assets[0]
        assert Path(historical.content.source).is_absolute()
        assert Path(historical.content.source).is_relative_to(first.manifest.parent)
        assert session.resources.get(asset).value.pixels.rgba == pixels_a
        assert session.project.diagnose().wait().diagnostics == ()
        session.redo().wait()
        assert session.resources.get(asset).value.pixels.rgba == pixels_b
        assert original.pixels().rgba == pixels_a

        # A disk capture pins bytes; a new read still verifies current disk data.
        captured = leases.enter_context(session.resources.snapshot(asset))
        value = session.modeling.records(m.ModelingQuery(assets=(asset,))).value.assets[0]
        relative = Path(value.content.source)
        assert not relative.is_absolute() and ".." not in relative.parts
        disk = saved.manifest.parent / relative
        png = disk.read_bytes()
        assert hashlib.sha256(png).hexdigest() == value.content.sha256
        before = session.info()
        disk.write_bytes(b"changed after image snapshot")
        try:
            rejected(lambda: session.resources.get(asset))
            assert captured.pixels().rgba == pixels_b
            assert original.pixels().rgba == pixels_a
            assert session.project.diagnose().wait().diagnostics
            assert session.info() == before
        finally:
            disk.write_bytes(png)
        assert session.resources.get(asset).value.pixels.rgba == pixels_b
        assert session.project.diagnose().wait().diagnostics == ()

        # A failed import or write cannot publish a partial project/candidate.
        invalid = folder / "invalid.psd"
        invalid.write_bytes(b"invalid PSD")
        job = session.project.replace(invalid, m.ImportFormat.PSD)
        error = rejected(job.wait)
        assert rejected(job.wait) is error
        assert session.info() == before
        def invalid_edit() -> None:
            with session.edit("Atomic attachment path validation") as edit:
                edit.meshes.rename(mesh, "Must roll back")
                attachment(edit, m.AttachmentEditCreate(id=uid(7), path="../escape.bin"))
        rejected(invalid_edit, "INVALID_ATTACHMENT_PATH")
        assert session.info() == before
        assert session.meshes.get(mesh).value.name == "Face"
        assert not (folder / "escape.bin").exists()

        # Reopening changes generation and invalidates pre-open identities/plans.
        reference = leases.enter_context(session.reference("asset", asset))
        plan = leases.enter_context(session.plan(resources.RenameImage(asset, "Stale", expected_name="Texture")))
        report = session.project.open(saved.manifest).wait()
        assert report.after.generation > report.before.generation
        rejected(reference.resolve, "STALE_HANDLE")
        stale = session.edit("Old generation plan")
        rejected(lambda: stale.apply(plan), "STALE_PLAN")
        assert session.info().history == (0, 0)
        assert session.resources.get(asset).value.name == "Texture"
        assert original.pixels().rgba == pixels_a and captured.pixels().rgba == pixels_b

        # The package's declared sidecar and pixels must match actual files.
        before = session.info()
        package = folder / "package"
        exported = session.project.export(package, m.ExportFormat.PACKAGE).wait()
        assert exported.published and exported.durable == (os.name != 'nt')
        assert session.info() == before
        model = json.loads((package / "model.model3.json").read_text())
        data_path = model["FileReferences"]["UserData"]
        assert data_path == "extras/data.userdata3.json"
        assert (package / data_path).read_bytes() == content
        texture_path = model["FileReferences"]["Textures"][0]
        with runtime.resources.load_png(package / texture_path).wait() as texture:
            assert texture.pixels().rgba == pixels_b
        imported = session.project.replace(package / "model.model3.json", m.ImportFormat.MODEL3).wait()
        assert imported.after.generation > imported.before.generation
        ids = session.modeling.records(m.ModelingQuery()).value.ids
        restored = session.modeling.records(m.ModelingQuery(attachments=ids["attachments"]))
        assert len(restored.value.attachments) == 1
        assert restored.value.attachments[0].path == data_path
        assert restored.value.attachments[0].bytes == content
        # Runtime imports normalize atlas pages into independent mesh artwork.
        # The exported page above remains byte-exact; after import the triangle's
        # painted interior is preserved and unused page pixels are transparent.
        restored_mesh = session.modeling.records(m.ModelingQuery(meshes=(ids["meshes"][0],))).value.meshes[0]
        assert isinstance(restored_mesh.texture_asset_id, m.SceneReferenceResolved)
        imported_asset = restored_mesh.texture_asset_id.object_id
        assert imported_asset in ids["assets"]
        artwork = session.resources.get(imported_asset).value.pixels
        assert (artwork.width, artwork.height) == (width, height)
        for x, y in ((width // 4, 3 * height // 4), (width // 8, height // 2)):
            offset = (y * width + x) * 4
            assert artwork.rgba[offset:offset + 4] == bytes((65, 43, 21, 255))
        for x, y in ((3 * width // 4, height // 4), (7 * width // 8, height // 2)):
            offset = (y * width + x) * 4
            assert artwork.rgba[offset:offset + 4] == bytes(4)
        assert captured.pixels().rgba == pixels_b
        assert session.info().history == (0, 0)
    usage = runtime.resources.usage()
    assert usage.prepared_images == baseline.prepared_images
    assert usage.plans == baseline.plans
    return dict(attachment_bytes=len(content), attachment_sha256=hashlib.sha256(content).hexdigest(),
                rgba_bytes=len(pixels_b), rgba_sha256=hashlib.sha256(pixels_b).hexdigest(),
                preserved_empty_and_unavailable=True, initial_history=saved_history,
                relocated_history=history, generation_replacements=3, package_sidecar=data_path,
                independent_leases_released=True)
