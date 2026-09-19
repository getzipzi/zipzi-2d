#!/usr/bin/env python3
"""Build and check the native Kasane path without a Cubism SDK or model."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DEMO = ROOT / "demos/kasane-preview"
BUILD = ROOT / "target/kasane"


def run(args: list[str], *, cwd: Path = ROOT, marker: str | None = None,
        log: str | None = None, timeout: int = 180,
        expected_errors: tuple[str, ...] = ()) -> None:
    print("Running:", " ".join(map(str, args)), flush=True)
    process = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, timeout=timeout)
    if log:
        (BUILD / log).write_text(process.stdout)
    failed = process.returncode != 0 or (marker is not None and marker not in process.stdout)
    error_lines = tuple(line for line in process.stdout.splitlines()
                        if line.startswith(("SCRIPT ERROR:", "ERROR:")))
    if error_lines != expected_errors or "ObjectDB instance" in process.stdout:
        failed = True
    if failed or not log or marker:
        print(process.stdout, end="")
    else:
        print(f"Output: {BUILD / log}", flush=True)
    if failed:
        raise SystemExit(f"Command failed (exit {process.returncode}); expected marker: {marker}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--render", action="store_true", help="Also test real rendering; opens a temporary window")
    parser.add_argument("--demo", action="store_true", help="Open the animated preview after checks")
    parser.add_argument("--godot", default=os.environ.get("GODOT_BIN"))
    parser.add_argument("--scons-python", default=os.environ.get("SCONS_PYTHON"))
    args = parser.parse_args()
    BUILD.mkdir(parents=True, exist_ok=True)
    godot = args.godot or shutil.which("godot") or shutil.which("godot4")
    fallback = Path("/Applications/Godot_mono.app/Contents/MacOS/Godot")
    if not godot and fallback.is_file():
        godot = str(fallback)
    if not godot:
        parser.error("Specify --godot or GODOT_BIN pointing to Godot 4.3+.")
    if not args.skip_build:
        run(["cmake", "-S", str(ROOT / "modules/kasane-core"), "-B", str(BUILD / "core"), "-DCMAKE_BUILD_TYPE=Debug"], log="configure.log")
        run(["cmake", "--build", str(BUILD / "core"), "-j8"], log="core-build.log")
        local_python = ROOT / "modules/gd-cubism/.venv/bin/python"
        python = args.scons_python or (str(local_python) if local_python.exists() else sys.executable)
        target_platform = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(platform.system())
        if not target_platform:
            parser.error("Unsupported build host")
        arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x86_64"
        run([python, "-m", "SCons", f"platform={target_platform}", f"arch={arch}", "target=template_debug", "-j8"],
            cwd=ROOT / "modules/gd-kasane", log="extension-build.log", timeout=1200)
    run(["ctest", "--test-dir", str(BUILD / "core"), "--output-on-failure"], log="core-tests.log")
    run([godot, "--headless", "--editor", "--path", str(DEMO), "--quit-after", "3"], log="import.log")
    run([godot, "--headless", "--path", str(DEMO), "--script", "res://tests/integration_test.gd"],
        marker="KASANE_INTEGRATION_TEST_OK", log="integration.log")
    run([godot, "--headless", "--path", str(DEMO), "--script", "res://tests/script_test.gd"],
        marker="KASANE_SCRIPT_TEST_OK", log="script-tests.log")
    expected_runtime_error = "SCRIPT ERROR: Invalid call. Nonexistent function 'deliberate_runtime_error' in base 'Nil'."
    run([godot, "--headless", "--path", str(DEMO), "--script", "res://tests/script_error_test.gd"],
        marker="KASANE_SCRIPT_ERROR_TEST_OK", log="script-errors.log",
        expected_errors=(expected_runtime_error, expected_runtime_error))
    if args.render:
        run([godot, "--path", str(DEMO), "--script", "res://tests/render_test.gd"],
            marker="KASANE_RENDER_TEST_OK", log="render.log")
    if args.demo:
        subprocess.Popen([godot, "--path", str(DEMO)], cwd=ROOT)


if __name__ == "__main__":
    main()
