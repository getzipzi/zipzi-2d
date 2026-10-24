"""Capture the imported model used by the independent Godot/wgpu image reference."""

import hashlib
import json
from pathlib import Path
import sys

import zipzi


DOCUMENT = "30000000-0000-4000-8000-000000000001"


def run(model3: Path, output: Path) -> Path:
    if not model3.is_absolute() or not output.is_absolute():
        raise ValueError("Model3 and output paths must be absolute")
    session = zipzi.Session(DOCUMENT, 100, 100, (50, 50))
    imported = session.import_model3(model3)
    if {(issue.asset_id, issue.code) for issue in imported.diagnostics} != {
        ("Physics", "MISSING_PHYSICS_ATTACHMENT"),
        ("motions/idle.motion3.json", "MISSING_MOTION_ATTACHMENT"),
    }:
        raise RuntimeError("External model resources are incomplete")
    missing = session.missing_attachments()
    if set(missing) != {
        "MISSING_MOTION_ATTACHMENT: motions/idle.motion3.json", "Physics: model.physics3.json",
    }:
        raise RuntimeError("External model attachment diagnostics changed")
    with session.edit("discard the reference fixture's missing attachments") as edit:
        for attachment in missing:
            edit.discard_missing_attachment(attachment)
    if session.diagnose_resources():
        raise RuntimeError("External model resources are incomplete")
    canvas = session.source_canvas
    if canvas is None:
        raise RuntimeError("Imported model has no source canvas")
    # The pinned reference renders source pixels from its top-left origin.
    # The SDK renders centered model coordinates, so cover the same source canvas.
    roi = (-canvas.origin_x, canvas.origin_y - canvas.height,
           canvas.width - canvas.origin_x, canvas.origin_y)
    output.mkdir(parents=True, exist_ok=True)
    with zipzi.Observer(256, 256, 256) as observer:
        scene = observer.capture_scene(session)
        frame = observer.render_scene(scene, roi=roi, resolution=(256, 256)).frame
    if len(frame.drawable_bounds) != 1 or frame.drawable_bounds[0].bounds is None:
        raise RuntimeError("Reference model has no single visible mesh crop")
    image = output / "frame.png"
    frame.save_png(image)
    report = output / "report.json"
    report.write_text(json.dumps({
        "status": "render_complete", "image_path": str(image),
        "image_sha256": hashlib.sha256(frame.png).hexdigest(),
        "crop_bounds": frame.drawable_bounds[0].bounds,
        "view": {"width": frame.width, "height": frame.height,
                 "scale": frame.view_scale, "offset": frame.view_offset},
        "source_canvas": {"origin_x": canvas.origin_x, "origin_y": canvas.origin_y},
        "adapter": {"name": frame.adapter_name, "backend": frame.adapter_backend},
    }, indent=2) + "\n", encoding="utf-8")
    return report


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: python_reference_capture.py /absolute/model3 /absolute/output")
    print(run(Path(sys.argv[1]), Path(sys.argv[2])))
