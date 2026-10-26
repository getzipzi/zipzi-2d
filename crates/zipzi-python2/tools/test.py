#!/usr/bin/env python3
"""Build zipzi-python2 and validate its typed API against local and IPC hosts.

This does not install/replace either production distribution. Run with uv's
CPython 3.14. Native artifacts are copied before testing, never loaded in place
from Cargo's mutable output directory.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig
from _support import exclusive_run

ROOT = Path(__file__).resolve().parents[3]

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tests", nargs="*", help="Optional unittest names, e.g. PythonSdk.test_shared_recipe")
    parser.add_argument("--release", action="store_true", help="Build the native adapter in Release for P0 measurements")
    parser.add_argument("--instrumented", action="store_true", help="Build separate P0 allocator/codec/copy probes; never use for latency")
    parser.add_argument("--build-only", action="store_true", help="Copy paired native/CPU artifacts and metadata without running previously accepted tests")
    parser.add_argument("--output", type=Path, help="Separate artifact directory (requires --build-only)")
    parser.add_argument("--recovery", action="store_true", help="Run focused P3A-1 complete-call/Edit/Job recovery tests; optional P3ARecovery.test_name selectors")
    parser.add_argument("--discovery", action="store_true", help="Run focused collection/Session/offline-catalog regressions; optional P3ADiscovery.test_name selectors")
    args = parser.parse_args()
    if args.output is not None and not args.build_only:
        parser.error('--output requires --build-only')
    if sys.version_info[:2] != (3, 14):
        parser.error("Run with uv run --locked python (CPython 3.14)")
    common = ROOT / "target/python2"
    output = args.output.resolve() if args.output is not None else common / "probes" if args.instrumented else common
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, PYO3_PYTHON=sys.executable, PYO3_BUILD_EXTENSION_MODULE="1")
    for name in ("PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"):
        env.pop(name, None)
    with exclusive_run(common / "runner.lock"):
        subprocess.run([sys.executable, str(ROOT / "crates/zipzi-python2/tools/generate.py"), "--check"], cwd=ROOT, check=True)
        command = ["cargo", "rustc", "--locked", "-p", "zipzi-python2", "--lib",
            "--message-format=json-render-diagnostics", "--features", "test-support,bench-instrumentation" if args.instrumented else "test-support"]
        if args.release:
            command.append("--release")
        if sys.platform == "darwin":
            # Like Maturin: bind CPython symbols from the importing interpreter.
            # Apply to this cdylib only, not every Rust dependency/executable.
            command += ["--", "-C", "link-arg=-Wl,-undefined,dynamic_lookup"]
        process = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
        (output / "build.jsonl").write_text(process.stdout)
        (output / "build.log").write_text(process.stderr)
        if process.returncode:
            sys.stderr.write(process.stderr[-8000:])
        process.check_returncode()
        artifacts = [row for line in process.stdout.splitlines()
            if (row := json.loads(line)).get("reason") == "compiler-artifact"
            and row.get("target", {}).get("name") == "zipzi_python2_native"]
        libraries = {Path(filename) for row in artifacts for filename in row["filenames"]
            if Path(filename).suffix in {".so", ".dylib", ".dll"}}
        if len(libraries) != 1:
            raise RuntimeError(f"Expected one native library, found {libraries}")
        native = output / "native/zipzi_native"
        native.mkdir(parents=True, exist_ok=True)
        (native / "__init__.py").write_text('"""Source validation fixture for zipzi-python2-native."""\n')
        extension = native / ("_native" + sysconfig.get_config_var("EXT_SUFFIX"))
        shutil.copy2(libraries.pop(), extension)
        host_command = ["cargo", "build", "--locked", "-p", "zipzi-runtime", "--features",
            "test-support,bench-instrumentation" if args.instrumented else "test-support", "--example", "local_host_fixture"]
        if args.release:
            host_command.append("--release")
        subprocess.run(host_command, cwd=ROOT, env=env, check=True)
        host = output / "local_host_fixture"
        shutil.copy2(ROOT / "target" / ("release" if args.release else "debug") / "examples/local_host_fixture", host)
        (output / "build-metadata.json").write_text(json.dumps(dict(profile=artifacts[0]["profile"],
            instrumented=args.instrumented, host_profile="release" if args.release else "debug",
            host_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),
            python=sys.version, artifact_sha256=hashlib.sha256(extension.read_bytes()).hexdigest()), indent=2) + "\n")
        if args.build_only:
            print(f'zipzi-python2: copied paired build artifacts to {output}')
            return
        env.update(ZIPZI_PYTHON2_NATIVE=str(native.parent), ZIPZI_PYTHON2_HOST=str(host))
        module = "test_p3a_discovery.py" if args.discovery else "test_p3a_recovery.py" if args.recovery else "test_api.py"
        subprocess.run([sys.executable, "-I", str(ROOT / "crates/zipzi-python2/tests" / module), "-v", *args.tests], cwd="/tmp", env=env, check=True)
        if not args.tests and not args.recovery and not args.discovery:
            subprocess.run([sys.executable, "-I", str(ROOT / "crates/zipzi-python2/tests/test_p3a_recovery.py"), "-v"], cwd="/tmp", env=env, check=True)
        subprocess.run(["uv", "run", "--locked", "pyright", "--project", str(ROOT / "crates/zipzi-python2/pyrightconfig.json")], cwd=ROOT, check=True)
        invalid = subprocess.run(["uv", "run", "--locked", "--project", str(ROOT), "pyright",
            "--project", str(ROOT / "crates/zipzi-python2/pyrightconfig.json"), "--outputjson", "--pythonpath", sys.executable,
            str(ROOT / "crates/zipzi-python2/tests/type_invalid.py")], cwd=ROOT, capture_output=True, text=True)
        if not invalid.stdout:
            raise RuntimeError(f"Static checker failed before producing diagnostics: {invalid.stderr}")
        diagnostics = json.loads(invalid.stdout)
        actual = {(Path(d["file"]).name, d["range"]["start"]["line"], d.get("rule"))
            for d in diagnostics["generalDiagnostics"] if d["severity"] == "error"}
        expected = {("type_invalid.py", line, rule.strip()) for line, value in
            enumerate((ROOT / "crates/zipzi-python2/tests/type_invalid.py").read_text().splitlines()) if "# error: " in value
            for rule in value.split("# error: ", 1)[1].split(",")}
        if invalid.returncode != 1 or diagnostics["summary"]["errorCount"] != 20 or actual != expected:
            raise RuntimeError(f"Expected twenty precise public typing diagnostics: {invalid.stdout}\n{invalid.stderr}")
        recovery_types = ROOT / "crates/zipzi-python2/tests/type_recovery_invalid.py"
        invalid = subprocess.run(["uv", "run", "--locked", "pyright", "--project",
            str(ROOT / "crates/zipzi-python2/pyrightconfig.json"), "--outputjson", str(recovery_types)],
            cwd=ROOT, capture_output=True, text=True)
        diagnostics = json.loads(invalid.stdout)
        expected = {(line, value.split("# error: ", 1)[1].strip()) for line, value in
            enumerate(recovery_types.read_text().splitlines()) if "# error: " in value}
        actual = {(d["range"]["start"]["line"], d.get("rule")) for d in diagnostics["generalDiagnostics"]
                  if d["severity"] == "error"}
        if invalid.returncode != 1 or not expected or actual != expected or diagnostics["summary"]["errorCount"] != len(expected):
            raise RuntimeError(f"Expected precise recovery/result typing failures: {invalid.stdout}\n{invalid.stderr}")
        print("zipzi-python2: Local/Connected behavior and positive/negative static types passed")

if __name__ == "__main__":
    main()
