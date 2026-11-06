#!/usr/bin/env python3
"""Build zipzi-python and validate its typed API against local and IPC hosts.

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
import tempfile
from _support import exclusive_run, executable, executable_name

ROOT = Path(__file__).resolve().parents[3]

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tests", nargs="*", help="Optional unittest names, e.g. LocalLifecycle.test_competing_drafts_history_and_exact_query_source")
    parser.add_argument("--release", action="store_true", help="Build the native adapter in Release for P0 measurements")
    parser.add_argument("--instrumented", action="store_true", help="Build separate P0 allocator/codec/copy probes; never use for latency")
    parser.add_argument("--build-only", action="store_true", help="Copy paired native/CPU artifacts and metadata without running previously accepted tests")
    parser.add_argument("--output", type=Path, help="Separate artifact directory (requires --build-only)")
    focus = parser.add_mutually_exclusive_group()
    focus.add_argument("--observation", action="store_true", help="Run explicit GPU observation/inspection acceptance; requires a usable device")
    focus.add_argument("--recovery", action="store_true", help="Run same-request recovery tests")
    focus.add_argument("--discovery", action="store_true", help="Run source-bound pagination and Session lifecycle tests")
    args = parser.parse_args()
    if args.output is not None and not args.build_only:
        parser.error('--output requires --build-only')
    if sys.version_info[:2] != (3, 14):
        parser.error("Run with uv run --locked python (CPython 3.14)")
    common = ROOT / "target/python"
    output = args.output.resolve() if args.output is not None else common / "probes" if args.instrumented else common
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, PYO3_PYTHON=sys.executable, PYO3_BUILD_EXTENSION_MODULE="1")
    for name in ("PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"):
        env.pop(name, None)
    with exclusive_run(common / "runner.lock"):
        subprocess.run([sys.executable, str(ROOT / "crates/zipzi-python/tools/generate.py"), "--check"], cwd=ROOT, check=True)
        command = ["cargo", "rustc", "--locked", "-p", "zipzi-python", "--lib",
            "--message-format=json-render-diagnostics", "--features", "test-support,bench-instrumentation" if args.instrumented else "test-support"]
        if args.release:
            command.append("--release")
        if sys.platform == "darwin":
            # Like Maturin: bind CPython symbols from the importing interpreter.
            # Apply to this cdylib only, not every Rust dependency/executable.
            command += ["--", "-C", "link-arg=-Wl,-undefined,dynamic_lookup"]
        process = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, encoding='utf-8')
        (output / "build.jsonl").write_text(process.stdout)
        (output / "build.log").write_text(process.stderr)
        if process.returncode:
            sys.stderr.write(process.stderr[-8000:])
        process.check_returncode()
        artifacts = [row for line in process.stdout.splitlines()
            if (row := json.loads(line)).get("reason") == "compiler-artifact"
            and row.get("target", {}).get("name") == "zipzi_python_native"]
        libraries = {Path(filename) for row in artifacts for filename in row["filenames"]
            if Path(filename).suffix in {".so", ".dylib", ".dll"}}
        if len(libraries) != 1:
            raise RuntimeError(f"Expected one native library, found {libraries}")
        native = output / "native/zipzi_native"
        native.mkdir(parents=True, exist_ok=True)
        (native / "__init__.py").write_text('"""Source validation fixture for zipzi-python-native."""\n')
        extension = native / ("_native" + sysconfig.get_config_var("EXT_SUFFIX"))
        shutil.copy2(libraries.pop(), extension)
        host_command = ["cargo", "build", "--locked", "-p", "zipzi-runtime", "--features",
            "gpu-observation,test-support,bench-instrumentation" if args.instrumented else "gpu-observation,test-support", "--example", "local_host_fixture", '--message-format=json-render-diagnostics']
        if args.release:
            host_command.append("--release")
        build = subprocess.run(host_command, cwd=ROOT, env=env, check=True, stdout=subprocess.PIPE, text=True, encoding='utf-8')
        source_host = executable(build.stdout, 'local_host_fixture', 'example')
        host = output / executable_name('local_host_fixture')
        if not host.exists() or hashlib.sha256(host.read_bytes()).digest() != hashlib.sha256(source_host.read_bytes()).digest():
            shutil.copy2(source_host, host)
        (output / "build-metadata.json").write_text(json.dumps(dict(profile=artifacts[0]["profile"],
            instrumented=args.instrumented, host_profile="release" if args.release else "debug",
            host_path=str(host), host_sha256=hashlib.sha256(host.read_bytes()).hexdigest(),
            python=sys.version, artifact_sha256=hashlib.sha256(extension.read_bytes()).hexdigest()), indent=2) + "\n")
        if args.build_only:
            print(f'zipzi-python: copied paired build artifacts to {output}')
            return
        env.update(
            ZIPZI_KERNEL_SOURCE_CLIENT=str(ROOT / 'crates/zipzi-python/client/python'),
            ZIPZI_KERNEL_NATIVE=str(native.parent),
            ZIPZI_KERNEL_HOST=str(host),
        )
        env.pop('ZIPZI_KERNEL_EDITOR_HOST', None)
        tests = args.tests
        if not tests:
            if args.recovery:
                tests = ['Recovery']
            elif args.observation:
                tests = [f'{backend}.test_observation_uses_explicit_view_and_bulk_png'
                         for backend in ('LocalLifecycle', 'ConnectedLifecycle')]
            elif args.discovery:
                tests = [f'{backend}.{case}'
                         for backend in ('LocalLifecycle', 'ConnectedLifecycle')
                         for case in ('test_pagination_rejects_an_old_source_after_publication',
                                      'test_detach_preserves_dirty_session_until_explicit_close')]
        subprocess.run([sys.executable, '-I', str(ROOT / 'crates/zipzi-python/tests/test_kernel.py'),
                        '-v', *tests], cwd=tempfile.gettempdir(), env=env, check=True)
        if not tests:
            subprocess.run([sys.executable, '-I', str(ROOT / 'crates/zipzi-python/tests/test_native_publication.py'), '-v'],
                           cwd=tempfile.gettempdir(), env=env, check=True)
            subprocess.run([sys.executable, str(ROOT / 'tools/test_tools.py'),
                            '-p', 'test_python_codegen.py'], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
