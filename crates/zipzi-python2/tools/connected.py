#!/usr/bin/env python3
"""Build/install the pure new client and run isolated connection acceptance."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
from _support import exclusive_run, executable, publish_executable, venv_python

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = ROOT / "crates/zipzi-python2"


def install(output: Path) -> Path:
    wheels = output / "wheels"
    subprocess.run(["uv", "build", "--wheel", "--out-dir", str(wheels), str(PACKAGE / "client")], cwd=ROOT, check=True, stdout=sys.stderr)
    environment = output / "venv"
    if not environment.exists():
        subprocess.run(["uv", "venv", "--python", "3.14", str(environment)], cwd=ROOT, check=True, stdout=sys.stderr)
    python = venv_python(environment)
    version = tomllib.loads((PACKAGE / "client/pyproject.toml").read_text(encoding='utf-8'))["project"]["version"]
    subprocess.run(["uv", "pip", "install", "--python", str(python), "--reinstall",
        str(wheels / f"zipzi-{version}-py3-none-any.whl")], cwd=ROOT, check=True, stdout=sys.stderr)
    return python


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install-only", action="store_true", help="Print the isolated interpreter for Editor acceptance")
    parser.add_argument("--editor-host", type=Path, help="Also run real desktop cold-start acceptance")
    parser.add_argument("tests", nargs="*", help="Optional InstalledConnection test names")
    args = parser.parse_args()
    output = ROOT / "target/python2/connected"
    output.mkdir(parents=True, exist_ok=True)
    with exclusive_run(output / "runner.lock"):
        python = install(output)
        if args.install_only:
            print(python)
            return
        subprocess.run([str(python), '-I', str(PACKAGE / 'tests/installed_contract.py'),
            '--output', str(output / 'installed-contract.md')], cwd=tempfile.gettempdir(), check=True)
        build = subprocess.run(["cargo", "build", "--locked", "-p", "zipzi-runtime", "--features", "gpu-observation,test-support",
            "--example", "local_host_fixture", '--message-format=json-render-diagnostics'], cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True, encoding='utf-8')
        host = publish_executable(output, executable(build.stdout, 'local_host_fixture', 'example'))
        environment = dict(os.environ, ZIPZI_PYTHON2_HOST=str(host))
        for name in ("PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"):
            environment.pop(name, None)
        subprocess.run([str(python), "-I", str(PACKAGE / "tests/test_connection.py"), "-v", *args.tests],
            cwd=tempfile.gettempdir(), env=environment, check=True)
        if args.editor_host:
            if not args.editor_host.is_absolute():
                parser.error("--editor-host must be absolute")
            environment["ZIPZI_PYTHON2_EDITOR"] = str(args.editor_host)
            subprocess.run([str(python), "-I", str(PACKAGE / "tests/test_editor_connection.py"), "-v"],
                cwd=tempfile.gettempdir(), env=environment, check=True)


if __name__ == "__main__":
    main()
