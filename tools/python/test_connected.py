#!/usr/bin/env python3
"""Build a CPU host fixture and verify an isolated pure client installation."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
from build_zipzi_client import ROOT, build


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tests", nargs="*", help="Optional unittest names (Connected.test_...)")
    parser.add_argument("--native-python", type=Path, help="Also compare the same recipe with an installed native wheel/interpreter")
    options = parser.parse_args()
    subprocess.run(["cargo", "build", "--locked", "-p", "zipzi-runtime", "--example", "local_host_fixture"], cwd=ROOT, check=True)
    wheel = build(ROOT / "target/python-client-wheels")
    environment = ROOT / "target/python-tests/connected"
    subprocess.run(["uv", "venv", "--allow-existing", "--python", "3.14", str(environment)], check=True)
    subprocess.run(["uv", "pip", "install", "--reinstall", "--python", str(environment), str(wheel)], check=True)
    host = ROOT / "target/debug/examples/local_host_fixture"
    env = dict(os.environ, ZIPZI_TEST_HOST=str(host))
    env.pop("PYTHONPATH", None)
    subprocess.run([str(environment / "bin/python"), "-I", str(ROOT / "crates/zipzi-python/tests/test_connected.py"), "-v", *options.tests], cwd="/tmp", env=env, check=True)
    if options.native_python:
        recipe = ROOT / "crates/zipzi-python/tests/connected_recipe.py"
        outputs = [subprocess.check_output([str(python), "-I", str(recipe), mode], cwd="/tmp", env=env, text=True)
            for python, mode in [(environment / "bin/python", "connected"), (options.native_python.absolute(), "native")]]
        assert json.loads(outputs[0]) == json.loads(outputs[1]), outputs
        print("NativeBackend/ConnectedBackend shared recipe: equivalent geometry, parameters, errors, history and save state")


if __name__ == "__main__":
    main()
