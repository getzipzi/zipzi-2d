"""Prepare the source SDK environment, then replace this process with Python.

All arguments belong to Python: script.py, -c, -m, -, or no arguments (REPL).
Build/install subprocesses never read the caller's stdin or write to stdout.
"""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import venv

ROOT = Path(__file__).resolve().parents[2]


def run(command, **kwargs):
    return subprocess.run(command, stdin=subprocess.DEVNULL, check=True, **kwargs)


def publish(source: Path, destination: Path):
    """Replace atomically; never truncate an extension loaded by another process."""
    if destination.exists():
        with source.open("rb") as incoming, destination.open("rb") as installed:
            if hashlib.file_digest(incoming, "sha256").digest() == hashlib.file_digest(installed, "sha256").digest():
                return
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=destination.parent, prefix=".zipzi-")
    os.close(fd)
    temporary = Path(temporary)
    try:
        shutil.copy2(source, temporary)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def prepare(root: Path, environment: dict[str, str]) -> Path:
    state = root / "target/python2/dev"
    state.mkdir(parents=True, exist_ok=True)
    python = state / "venv/bin/python"
    # Serialize environment creation, editable installation and artifact publication.
    # Release before running user code, so independent scripts can run concurrently.
    with (state / "prepare.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not python.exists():
            venv.EnvBuilder(with_pip=False).create(python.parent.parent)
        info = json.loads(run([
            str(python), "-I", "-c",
            "import json,sys,sysconfig; print(json.dumps({"
            "'version':list(sys.version_info[:2]),"
            "'site':sysconfig.get_path('platlib'),"
            "'suffix':sysconfig.get_config_var('EXT_SUFFIX')}))",
        ], env=environment, capture_output=True, text=True).stdout)
        if info["version"] != [3, 14]:
            raise RuntimeError(f"Expected CPython 3.14 in {python.parent.parent}; remove that environment and retry")
        package = root / "crates/zipzi-python2"
        run(["uv", "pip", "install", "--quiet", "--python", str(python), "--no-deps",
             "--editable", str(package / "client")],
            cwd=root, env=environment, stdout=sys.stderr)

        # A dedicated target keeps ordinary source builds separate from the SDK's
        # test-support and instrumented fixtures. Cargo checks freshness each run.
        command = ["cargo", "rustc", "--locked", "-p", "zipzi-python2", "--lib",
                   "--target-dir", str(state / "cargo"), "--message-format=json-render-diagnostics"]
        if sys.platform == "darwin":
            command += ["--", "-C", "link-arg=-Wl,-undefined,dynamic_lookup"]
        build_environment = dict(environment, PYO3_PYTHON=str(python), PYO3_BUILD_EXTENSION_MODULE="1")
        result = run(command, cwd=root, env=build_environment, stdout=subprocess.PIPE, text=True)
        libraries = {
            Path(filename)
            for line in result.stdout.splitlines()
            if (row := json.loads(line)).get("reason") == "compiler-artifact"
            and row.get("target", {}).get("name") == "zipzi_python2_native"
            for filename in row["filenames"]
            if Path(filename).suffix in {".so", ".dylib", ".dll"}
        }
        if len(libraries) != 1:
            raise RuntimeError(f"Expected one zipzi native library from Cargo, found {libraries}")
        native = Path(info["site"]) / "zipzi_native"
        publish(package / "native/zipzi_native/__init__.py", native / "__init__.py")
        publish(libraries.pop(), native / ("_native" + info["suffix"]))
    return python


def main():
    if sys.version_info[:2] != (3, 14):
        raise RuntimeError("Use tools/dev-python with the workspace's CPython 3.14")
    environment = dict(os.environ)
    # Avoid importing a different checkout/package via the caller's Python setup.
    # Runtime discovery variables are deliberately preserved for Connected use.
    for name in ("PYTHONPATH", "PYTHONHOME"):
        environment.pop(name, None)
    python = prepare(ROOT, environment)
    environment["VIRTUAL_ENV"] = str(python.parent.parent)
    environment["PATH"] = str(python.parent) + os.pathsep + environment.get("PATH", "")
    os.execve(str(python), [str(python), *sys.argv[1:]], environment)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(f"dev-python: preparation failed: {error.cmd}", file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        sys.exit(error.returncode if error.returncode > 0 else 128 - error.returncode)
    except (OSError, RuntimeError) as error:
        print(f"dev-python: {error}", file=sys.stderr)
        sys.exit(1)
