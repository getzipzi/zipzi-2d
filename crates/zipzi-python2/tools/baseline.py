#!/usr/bin/env python3
"""P0 measurements for zipzi-python2 itself; timing and Python probes are separate."""
import argparse
from collections import Counter
import gc
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time
import tracemalloc

PACKAGE = Path(__file__).resolve().parents[1]
ROOT = PACKAGE.parents[1]
CASES = ("small_query", "single_edit", "batch_100", "large_array")
PENDING = ("image", "plan", "cpu_sample")
SAMPLES, WARMUP = 32, 4

def fixture(zipzi, case):
    from zipzi import meshes, ops
    runtime = zipzi.local()
    session = runtime.sessions.create("79200000-0000-4000-8000-000000000001")
    count = 32 if case == "large_array" else 1
    points = tuple((float(x), float(y)) for y in range(count + 1) for x in range(count + 1))
    ids = tuple(range(len(points)))
    triangles = []
    for y in range(count):
        for x in range(count):
            a = y * (count + 1) + x
            b, d, c = a + 1, a + count + 1, a + count + 2
            triangles.extend(((a, b, c), (a, c, d)))
    with session.edit("fixture") as edit:
        face = edit.meshes.create(meshes.MeshSpec("Face", meshes.Geometry(ids, points, points, tuple(triangles))))
    shifted = [tuple((x + offset, y) for x, y in points) for offset in (0., .25)]
    def invoke(index):
        if case == "small_query":
            return session.meshes.get(face)
        with session.edit(case) as edit:
            if case == "single_edit":
                edit.meshes.rename(face, f"Face {index}")
            elif case == "batch_100":
                edit.apply_many(tuple(ops.Rename(face, f"Batch {index}-{n}") for n in range(100)))
            elif case == "large_array":
                edit.meshes.set_positions(face, vertex_ids=ids, positions=shifted[index % 2])
        return None
    def validate(last):
        snapshot = session.meshes.get(face)
        assert snapshot.geometry.vertex_ids == ids
        assert snapshot.geometry.triangles == tuple(triangles)
        assert snapshot.geometry.positions == (shifted[last % 2] if case == "large_array" else points)
        expected_name = f"Face {last}" if case == "single_edit" else f"Batch {last}-99" if case == "batch_100" else "Face"
        assert snapshot.name == expected_name
        writes = last if case == "large_array" else last + 1
        assert session.info().history == (1 if case == "small_query" else 1 + writes, 0)
    return runtime, invoke, validate, len(ids) * 12 if case == "large_array" else 0

def probe(invoke, index):
    calls = Counter()
    def profile(frame, event, value):
        if event == "c_call" and type(getattr(value, "__self__", None)).__name__ == "_Runtime":
            calls[f"native.{value.__name__}"] += 1
        if event == "return" and frame.f_globals.get("__name__") == "json" and frame.f_code.co_name in {"dumps", "loads"}:
            calls[f"python_json.{frame.f_code.co_name}"] += 1
    previous = sys.getprofile()
    try:
        sys.setprofile(profile)
        invoke(index)
    finally:
        sys.setprofile(previous)
    gc.collect()
    tracemalloc.start()
    try:
        base, _ = tracemalloc.get_traced_memory()
        tracemalloc.reset_peak()
        result = invoke(index + 1)
        live, peak = tracemalloc.get_traced_memory()
        del result
    finally:
        tracemalloc.stop()
    return dict(calls=dict(calls), python_live_bytes=max(0, live - base), python_peak_bytes=max(0, peak - base),
        copied_bytes=None, rust_json_calls=None, rust_live_bytes=None, rust_peak_bytes=None)

def worker(native):
    sys.path[:0] = [str(PACKAGE / "client/python"), str(native)]
    import zipzi
    import zipzi_native._native as extension
    cases = {}
    for case in CASES:
        runtime, invoke, validate, payload = fixture(zipzi, case)
        times = []
        try:
            for index in range(SAMPLES + WARMUP):
                start = time.perf_counter_ns()
                result = invoke(index)
                elapsed = time.perf_counter_ns() - start
                del result
                if index >= WARMUP:
                    times.append(elapsed)
            measured = probe(invoke, SAMPLES + WARMUP)
            validate(SAMPLES + WARMUP + 1)
            cases[case] = dict(wall_ns=times, input_payload_bytes=payload, probe=measured)
        finally:
            runtime.close()
    return dict(validated=True, python=sys.version, extension_sha256=hashlib.sha256(Path(extension.__file__).read_bytes()).hexdigest(), cases=cases)

def summarize(runs):
    if not runs or any(not r.get("validated") for r in runs):
        raise ValueError("Measurements require independent successful validation")
    result = {}
    for run in runs:
        if run["extension_sha256"] != runs[0]["extension_sha256"] or set(run["cases"]) != set(CASES):
            raise ValueError("Binary or workload changed")
    for case in CASES:
        medians, p95s = [], []
        for run in runs:
            values = sorted(run["cases"][case]["wall_ns"])
            if len(values) != SAMPLES or any(type(v) is not int or v <= 0 for v in values):
                raise ValueError("Missing or invalid timing samples")
            medians.append(statistics.median(values))
            p95s.append(values[math.ceil(.95 * len(values)) - 1])
        median = statistics.median(medians)
        result[case] = dict(p50_ns=median, p95_ns=statistics.median(p95s),
            p50_range_ns=[min(medians), max(medians)], relative_p50_range=(max(medians) - min(medians)) / median,
            probes=[r["cases"][case]["probe"] for r in runs])
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-dir", type=Path, default=ROOT / "target/python2/native")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args.native_dir.absolute()), sort_keys=True))
        return
    from _support import exclusive_run
    if args.output is None or args.runs < 1:
        parser.error("--output and positive --runs are required")
    with exclusive_run(args.native_dir.parent / "runner.lock"):
        measure(args, parser)

def measure(args, parser):
    metadata = json.loads((args.native_dir.parent / "build-metadata.json").read_text())
    if metadata["profile"]["debug_assertions"]:
        parser.error("Build the adapter with tools/test.py --release before measuring")
    args.output.mkdir(parents=True, exist_ok=False)
    runs = []
    for index in range(args.runs):
        process = subprocess.run([sys.executable, "-I", str(Path(__file__).absolute()), "--worker", "--native-dir", str(args.native_dir.absolute())],
            cwd="/tmp", env={k: v for k, v in os.environ.items() if k not in {"PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"}},
            capture_output=True, text=True)
        (args.output / f"run-{index}.json").write_text(process.stdout)
        (args.output / f"run-{index}.stderr").write_text(process.stderr)
        process.check_returncode()
        runs.append(json.loads(process.stdout))
    if any(r["extension_sha256"] != metadata["artifact_sha256"] for r in runs):
        raise RuntimeError("Loaded native artifact differs from the measured build")
    report = dict(schema=1, status="partial", build=metadata, runs=len(runs), samples=SAMPLES, warmup=WARMUP,
        measurements=summarize(runs), pending_workloads=PENDING,
        missing_metrics=["Rust codec counters", "actual copy bytes", "Rust live/peak heap", "frozen regression thresholds"])
    path = args.output / "summary.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(path)

if __name__ == "__main__":
    main()
