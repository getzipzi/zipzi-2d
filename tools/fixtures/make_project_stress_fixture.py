"""Expand a real Zipzi project while preserving its mesh/keyform relationships.

Usage: uv run --locked python tools/fixtures/make_project_stress_fixture.py SOURCE.zipzi OUTPUT.zipzi COPIES
COPIES includes the original set of meshes and bindings. Output must not exist.
This trusted-input benchmark tool does not replace native project admission.
"""

import copy
import hashlib
import json
import zipfile
from pathlib import Path
import sys
import uuid


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    source, output = (Path(arg).resolve() for arg in sys.argv[1:3])
    copies = int(sys.argv[3])
    if copies < 1:
        raise ValueError("COPIES must be positive")
    if output.exists():
        raise FileExistsError(output)
    with zipfile.ZipFile(source) as archive:
        format_data = json.loads(archive.read("format.json"))
        original_json = archive.read("project.json")
        if (format_data["format"] != "zipzi-project-package" or
                format_data["container_version"] != 1 or format_data["required_features"] or
                format_data["document_sha256"] != hashlib.sha256(original_json).hexdigest()):
            raise ValueError("Expected a native v1 project")
        input_data = json.loads(original_json)
        if input_data["format"] != "zipzi-project" or input_data["format_version"] != 22:
            raise ValueError("Expected native document v22")
        assets = {name: archive.read(name) for name in archive.namelist()
                  if name.startswith("assets/") and not name.endswith("/")}
        if any(name != f"assets/{hashlib.sha256(data).hexdigest()}.png"
               for name, data in assets.items()):
            raise ValueError("Resource hash mismatch")
    document = input_data["document"]
    original_meshes = list(document["meshes"])
    original_bindings = list(document["bindings"])
    if any(document.get(key) for key in ("blend_bindings", "glues", "draw_order_groups")):
        raise ValueError("Source has additional mesh relationships; choose a different fixture")
    if any(binding["mesh_id"] not in {m["id"] for m in original_meshes} for binding in original_bindings):
        raise ValueError("Binding references a missing mesh")

    for index in range(1, copies):
        ids = {m["id"]: str(uuid.uuid5(uuid.NAMESPACE_URL, f"zipzi-stress/{index}/{m['id']}"))
               for m in original_meshes}
        for original in original_meshes:
            mesh = copy.deepcopy(original)
            mesh["id"] = ids[original["id"]]
            mesh["runtime_id"] = f"{original['runtime_id']}_stress_{index}"
            mesh["name"] = f"{original['name']} stress {index}"
            mesh["properties"]["masks"] = [ids.get(mask, mask) for mask in mesh["properties"]["masks"]]
            document["meshes"].append(mesh)
        for original in original_bindings:
            binding = copy.deepcopy(original)
            binding["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, f"zipzi-stress/{index}/{original['id']}"))
            binding["mesh_id"] = ids[original["mesh_id"]]
            samples = {}
            for axis in binding["axes"]:
                axis["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, f"zipzi-stress/{index}/{axis['id']}"))
                for sample in axis["samples"]:
                    old_id = sample["id"]
                    sample["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, f"zipzi-stress/{index}/{old_id}"))
                    samples[old_id] = sample["id"]
            for keyform in binding["keyforms"]:
                keyform["samples"] = [samples[sample] for sample in keyform["samples"]]
            document["bindings"].append(binding)

    encoded = (json.dumps(input_data, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    format_data["document_sha256"] = hashlib.sha256(encoded).hexdigest()
    with zipfile.ZipFile(output, "x", allowZip64=True) as archive:
        for name, data in (("format.json", json.dumps(format_data).encode()),
                           ("project.json", encoded), *sorted(assets.items())):
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_STORED if name.startswith("assets/") else zipfile.ZIP_DEFLATED
            info.external_attr = 0o100600 << 16
            archive.writestr(info, data)
    print(f"{output}: {len(document['meshes'])} meshes, {len(document['bindings'])} bindings")


if __name__ == "__main__":
    main()
