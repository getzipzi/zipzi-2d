#!/usr/bin/env python3
"""Test current SDK sources with a persistent Debug install, or accept a Release wheel."""

import argparse
from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]


@contextmanager
def exclusive_run(path):
    """Editable installs must not replace a shared extension during another run."""
    with path.open("a+b") as lock:
        if os.name == "nt":
            import msvcrt
            if lock.tell() == 0:
                lock.write(b"\0")
                lock.flush()
            lock.seek(0)
            try:
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError as error:
                raise RuntimeError("Another Python SDK test run is active; retry after it finishes") from error
        else:
            import fcntl
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError("Another Python SDK test run is active; retry after it finishes") from error
        try:
            yield
        finally:
            if os.name == "nt":
                lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(lock, fcntl.LOCK_UN)


def execute(command, *, cwd, env):
    print("+ " + " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=cwd, env=env, check=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--observe", action="store_true", help="Build GPU observation and also run GPU tests")
    parser.add_argument("--wheel", action="store_true", help="Accept a packaged Release wheel instead of a Debug development install")
    parser.add_argument("test_arguments", nargs=argparse.REMAINDER, help="unittest arguments after --, for example -- -k history")
    args = parser.parse_args(argv)
    test_arguments = args.test_arguments
    if test_arguments[:1] == ["--"]:
        test_arguments = test_arguments[1:]
    if args.wheel and test_arguments:
        parser.error("Wheel acceptance runs the complete Python suite; filters are only available in development mode")

    # Development installs share an in-place extension, so every invocation
    # refreshes it before starting new Python processes. Wheel environments are
    # separate and import only their packaged files, including for CPU-only builds.
    directory = ROOT / "target/python-tests"
    directory.mkdir(parents=True, exist_ok=True)
    with exclusive_run(directory / "runner.lock"):
        run_tests(args, test_arguments, directory)


def run_tests(args, test_arguments, directory):
    flavor = "observe" if args.observe else "cpu"
    environment = directory / (f"wheel-{flavor}-venv" if args.wheel else "develop-venv")
    outside = directory / "run"
    outside.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    for key in ["VIRTUAL_ENV", "PYTHONPATH", "PYTHONHOME", "KASANE_DEV_SESSION_DIR"]:
        env.pop(key, None)
    env["UV_PROJECT_ENVIRONMENT"] = str(environment)
    # Share unchanged Rust dependencies with ordinary SDK tests. Cargo keeps
    # Debug/Release and feature variants separate inside the workspace target.
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    uv = ["uv", "run", "--locked", "--project", ROOT]
    manifest = ROOT / "crates/kasane-python/Cargo.toml"
    features = ["--features", "observe"] if args.observe else []

    if args.wheel:
        output = ROOT / "target/python-wheels" / flavor
        output.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="build-", dir=output) as temporary:
            execute([*uv, "maturin", "build", "--locked", "--release", "--manifest-path", manifest, "--out", temporary, *features], cwd=ROOT, env=env)
            wheels = list(Path(temporary).glob("*.whl"))
            if len(wheels) != 1:
                raise RuntimeError(f"Expected one SDK wheel, found {wheels}")
            wheel = output / wheels[0].name
            shutil.copy2(wheels[0], wheel)
        python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        execute(["uv", "pip", "install", "--python", python, "--reinstall", "--no-deps", wheel], cwd=outside, env=env)
    else:
        execute([*uv, "maturin", "develop", "--locked", "--uv", "--manifest-path", manifest, *features], cwd=ROOT, env=env)

    # Detect a stale/wrong feature variant before any passing CPU tests can
    # accidentally hide that the extension was not rebuilt or installed.
    expected = repr(args.observe)
    check = f"import kasane; actual = kasane.capabilities()['gpu_observation']; assert actual == {expected}, (actual, {expected})"
    execute([*uv, "python", "-c", check], cwd=outside, env=env)
    tests = ROOT / "crates/kasane-python/tests"
    patterns = ["test_cpu.py", "test_inspection.py"]
    if args.observe:
        patterns.append("test_observe.py")
    for pattern in patterns:
        execute([*uv, "python", "-m", "unittest", "discover", "-s", tests, "-p", pattern, *test_arguments], cwd=outside, env=env)
    if args.wheel:
        requirements = ["--require-gpu", "--require-image-reference"] if args.observe else []
        execute([*uv, "python", ROOT / "tools/validation/validate_sdk.py", "--wheel", wheel, "--output", directory / f"acceptance-{flavor}", *requirements], cwd=outside, env=env)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
    except RuntimeError as error:
        print(error, file=sys.stderr)
        raise SystemExit(2) from None
