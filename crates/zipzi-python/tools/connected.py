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
PACKAGE = ROOT / "crates/zipzi-python"


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
    parser.add_argument("tests", nargs="*", help="Optional v4 unittest names (ConnectedLifecycle or Recovery)")
    args = parser.parse_args()
    if args.editor_host and not args.editor_host.is_absolute():
        parser.error('--editor-host must be absolute')
    output = ROOT / "target/python/connected"
    output.mkdir(parents=True, exist_ok=True)
    with exclusive_run(output / "runner.lock"):
        python = install(output)
        if args.install_only:
            print(python)
            return
        build = subprocess.run(["cargo", "build", "--locked", "-p", "zipzi-runtime", "--features", "gpu-observation,test-support",
            "--example", "local_host_fixture", '--message-format=json-render-diagnostics'], cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True, encoding='utf-8')
        host = publish_executable(output, executable(build.stdout, 'local_host_fixture', 'example'))
        environment = dict(os.environ, ZIPZI_KERNEL_HOST=str(host))
        for name in ('PYTHONPATH', 'PYTHONHOME', 'ZIPZI_DEV_SESSION_DIR', 'ZIPZI_KERNEL_EDITOR_HOST',
                     'ZIPZI_KERNEL_SOURCE_CLIENT', 'ZIPZI_KERNEL_NATIVE'):
            environment.pop(name, None)
        command = [str(python), '-I', str(PACKAGE / 'tests/test_kernel.py'), '-v']
        subprocess.run([*command, *(args.tests or ['Values', 'ConnectedLifecycle', 'Recovery'])],
                       cwd=tempfile.gettempdir(), env=environment, check=True)
        if not args.tests:
            subprocess.run([str(python), '-I', str(PACKAGE / 'tests/test_connection.py'), '-v'],
                           cwd=tempfile.gettempdir(), env=environment, check=True)
            if os.name == 'nt':
                subprocess.run([str(python), '-I', str(PACKAGE / 'tests/test_windows_transport.py'), '-v'],
                               cwd=tempfile.gettempdir(), env=environment, check=True)
        if args.editor_host:
            environment['ZIPZI_KERNEL_EDITOR_HOST'] = str(args.editor_host)
            subprocess.run([*command, 'ConnectedLifecycle'],
                           cwd=tempfile.gettempdir(), env=environment, check=True)
            subprocess.run([str(python), '-I', str(PACKAGE / 'tests/editor_mesh_generation.py'), '-v'],
                           cwd=tempfile.gettempdir(), env=environment, check=True)


if __name__ == "__main__":
    main()
