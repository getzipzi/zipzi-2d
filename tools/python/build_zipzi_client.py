#!/usr/bin/env python3
"""Build a pure-Python zipzi client wheel from the shared public facade.

The native wheel is built by Maturin. This wheel contains the same Python API
without an extension; install one distribution variant in an environment.
"""
import argparse
import base64
import csv
import hashlib
import io
import subprocess
import sys
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/packaging'))
from release import metadata


def build(out: Path) -> Path:
    version = metadata()["python_version"]
    package = ROOT / "crates/zipzi-python"
    info = f"zipzi-{version}.dist-info"
    files = {f"zipzi/{p.name}": p.read_bytes() for p in (package / "python/zipzi").iterdir()
             if p.suffix == ".py" or p.name == "py.typed"}
    files[f"{info}/METADATA"] = (f"Metadata-Version: 2.4\nName: zipzi\nVersion: {version}\n"
        "Summary: Zipzi local connection client (optional native backend absent)\n"
        "Requires-Python: >=3.14,<3.15\nLicense-Expression: MIT\n").encode()
    files[f"{info}/WHEEL"] = b"Wheel-Version: 1.0\nGenerator: zipzi-client\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    for name in ("LICENSE-MIT", "THIRD_PARTY_NOTICES.md"):
        files[f"{info}/licenses/{name}"] = (package / name).read_bytes()
    rows = []
    for name, data in sorted(files.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
        rows.append([name, f"sha256={digest}", len(data)])
    record = io.StringIO(newline="")
    writer = csv.writer(record)
    writer.writerows(rows + [[f"{info}/RECORD", "", ""]])
    files[f"{info}/RECORD"] = record.getvalue().encode()
    out.mkdir(parents=True, exist_ok=True)
    target = out / f"zipzi-{version}-py3-none-any.whl"
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as wheel:
        for name, data in sorted(files.items()):
            entry = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            wheel.writestr(entry, data)
    return target


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=ROOT / "target/python-client-wheels")
    parser.add_argument("--install", action="store_true", help="Install into the isolated acceptance environment and print its interpreter")
    options = parser.parse_args()
    wheel = build(options.out_dir).resolve()
    if options.install:
        environment = ROOT / "target/python-tests/connected"
        subprocess.run(["uv", "venv", "--allow-existing", "--python", "3.14", str(environment)], check=True)
        subprocess.run(["uv", "pip", "install", "--reinstall", "--python", str(environment), str(wheel)], check=True)
        print(environment / "bin/python")
    else:
        print(wheel)
