#!/usr/bin/env python3
"""P0 measurements for zipzi-python2 itself; timing and Python probes are separate."""
import argparse
from contextlib import contextmanager
from dataclasses import asdict
import platform
import tempfile
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
CASES = ("small_query", "single_edit", "batch_100", "large_array", "image", "plan")
PENDING = ("cpu_sample",)
SAMPLES, WARMUP = 32, 4

def fixture(zipzi, case, runtime):
    from zipzi import meshes, ops
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
    image_size = 256
    payloads = tuple(bytes((color, 34, 56, 255)) * (image_size * image_size) for color in (12, 13))
    last_image = [None]
    shifted = [tuple((x + offset, y) for x, y in points) for offset in (0., .25)]
    def invoke(index):
        if case == "small_query":
            return session.meshes.get(face)
        if case == "image":
            with runtime.resources.prepare_image(image_size, image_size, payloads[index % 2]) as image:
                with session.edit(case) as edit:
                    last_image[0] = edit.resources.create_image(f"Image {index}", image)
            return None
        if case == "plan":
            with session.plan(meshes.Rename(face, f"Planned {index}", expected_name="Face" if index == 0 else f"Planned {index - 1}")) as plan:
                with session.edit(case) as edit:
                    edit.apply(plan)
            return None
        with session.edit(case) as edit:
            if case == "single_edit":
                edit.meshes.rename(face, f"Face {index}")
            elif case == "batch_100":
                edit.apply_many(tuple(meshes.Rename(face, f"Batch {index}-{n}") for n in range(100)))
            elif case == "large_array":
                edit.meshes.set_positions(face, vertex_ids=ids, positions=shifted[index % 2])
        return None
    def validate(last):
        snapshot = session.meshes.get(face)
        assert snapshot.value.geometry.vertex_ids == ids
        assert snapshot.value.geometry.triangles == tuple(triangles)
        assert snapshot.value.geometry.positions == (shifted[last % 2] if case == "large_array" else points)
        expected_name = f"Face {last}" if case == "single_edit" else f"Batch {last}-99" if case == "batch_100" else "Face"
        if case == "plan":
            expected_name = f"Planned {last}"
        assert snapshot.value.name == expected_name
        if case == "image":
            pixels = session.resources.get(last_image[0]).value.pixels
            assert (pixels.width, pixels.height, pixels.rgba) == (image_size, image_size, payloads[last % 2])
            assert session.resources.get(last_image[0]).value.name == f"Image {last}"
        usage = runtime.resources.usage()
        assert (usage.prepared_images, usage.plans) == (0, 0)
        writes = last if case == "large_array" else last + 1
        assert session.info().history == (1 if case == "small_query" else 1 + writes, 0)
    payload = image_size * image_size * 4 if case == "image" else len(ids) * 12 if case == "large_array" else 0
    return runtime, invoke, validate, payload

def probe(runtime, invoke, index, mode):
    def native(reset):
        if mode == "local":
            return runtime._backend._native._measure(reset)
        return runtime._backend._request(dict(op="diagnostics", reset=reset), runtime, "probe")
    # The connected control window includes its own response/request overhead;
    # retain an empty-window calibration instead of silently subtracting it.
    native(True)
    calibration = native(False)
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
    gc.collect()
    native(True)
    result = invoke(index + 2)
    rust = native(False)
    del result
    assert rust["enabled"] is True, "Rust probes are absent; build --instrumented"
    assert rust["rust_peak_bytes"] >= rust["rust_live_bytes"]
    assert rust["rust_peak_bytes"] >= rust["rust_start_bytes"]
    measured = dict(calls=dict(calls), python_live_bytes=max(0, live - base), python_peak_bytes=max(0, peak - base),
        rust=rust, empty_control_window=calibration, image_memory_after=asdict(runtime.resources.usage()))
    validate_probe(mode, measured)
    return measured

def validate_probe(mode, probe):
    rust = probe["rust"]
    required = ("rust_live_bytes", "rust_start_bytes", "rust_peak_bytes", "rust_allocations", "rust_allocated_bytes",
        "json_encodes", "json_decodes", "json_value_builds", "json_admission_encodes", "materialized_input_bytes", "copied_payload_bytes")
    if rust.get("enabled") is not True or any(type(rust.get(k)) is not int or rust[k] < 0 for k in required):
        raise ValueError("Rust probes require real, nonnegative counters")
    if rust["rust_peak_bytes"] < max(rust["rust_start_bytes"], rust["rust_live_bytes"]) or rust["rust_allocations"] == 0:
        raise ValueError("Invalid allocator probe")
    if mode == "local" and any(rust[k] != 0 for k in ("json_encodes", "json_decodes", "json_value_builds")):
        raise ValueError("Local transport introduced a JSON conversion")
    if mode == "connected" and any(rust[k] == 0 for k in ("json_encodes", "json_decodes")):
        raise ValueError("Connected probe did not observe actual wire codecs")
    if any(probe["image_memory_after"][k] != 0 for k in ("prepared_images", "plans", "preparing_bytes")):
        raise ValueError("A completed workload leaked preparation/plan owners")

@contextmanager
def execution(zipzi, mode, host):
    if mode == "local":
        with zipzi.local() as runtime:
            yield runtime
        return
    with tempfile.TemporaryDirectory(prefix="zzp0-", dir="/tmp") as folder:
        env = {k: v for k, v in os.environ.items() if not k.startswith("ZIPZI_TEST_")}
        env.update(ZIPZI_RUNTIME_DIR=folder, ZIPZI_RUNTIME_PROFILE="python2")
        child = subprocess.Popen([str(host)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while True:
                try:
                    runtime = zipzi.connect(endpoint=Path(folder) / "python2.sock")
                    break
                except (FileNotFoundError, ConnectionRefusedError):
                    if child.poll() is not None:
                        raise RuntimeError(child.stderr.read().decode())
                    if time.monotonic() >= deadline:
                        raise TimeoutError("CPU host startup")
                    time.sleep(.005)
            with runtime:
                yield runtime
        finally:
            child.terminate()
            child.wait(timeout=5)
            child.stderr.close()

def client_hash():
    digest = hashlib.sha256()
    for path in sorted((PACKAGE / "client/python").rglob("*.py")):
        digest.update(str(path.relative_to(PACKAGE)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()

def worker(native, mode, role):
    sys.path[:0] = [str(PACKAGE / "client/python"), str(native)]
    import zipzi
    metadata = json.loads((native.parent / "build-metadata.json").read_text())
    assert metadata["instrumented"] == (role == "probe")
    assert metadata["host_profile"] == "release"
    host = native.parent / "local_host_fixture"
    assert hashlib.sha256(host.read_bytes()).hexdigest() == metadata["host_sha256"]
    cases = {}
    for case in CASES:
        with execution(zipzi, mode, host) as rt:
            runtime, invoke, validate, payload = fixture(zipzi, case, rt)
            times = []
            for index in range(SAMPLES + WARMUP):
                start = time.perf_counter_ns()
                result = invoke(index)
                elapsed = time.perf_counter_ns() - start
                del result
                if index >= WARMUP:
                    times.append(elapsed)
            if role == "probe":
                measured = probe(runtime, invoke, SAMPLES + WARMUP, mode)
                validate(SAMPLES + WARMUP + 2)
                cases[case] = dict(input_payload_bytes=payload, probe=measured)
            else:
                validate(SAMPLES + WARMUP - 1)
                cases[case] = dict(wall_ns=times, input_payload_bytes=payload)
    extensions = list((native / "zipzi_native").glob("_native*.so"))
    assert len(extensions) == 1
    digest = hashlib.sha256(extensions[0].read_bytes()).hexdigest()
    assert digest == metadata["artifact_sha256"]
    return dict(validated=True, mode=mode, role=role, python=sys.version, platform=platform.platform(),
        client_sha256=client_hash(), measurement_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        extension_sha256=digest, host_sha256=metadata["host_sha256"], cases=cases)

def summarize(runs):
    if not runs or any(not r.get("validated") for r in runs):
        raise ValueError("Measurements require independent successful validation")
    result = {}
    for run in runs:
        if any(run[k] != runs[0][k] for k in ("extension_sha256", "client_sha256", "measurement_sha256")) or set(run["cases"]) != set(CASES):
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
            probes=[r["cases"][case]["probe"] for r in runs if "probe" in r["cases"][case]])
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-dir", type=Path, default=ROOT / "target/python2/native")
    parser.add_argument("--probe-native-dir", type=Path, default=ROOT / "target/python2/probes/native")
    parser.add_argument("--mode", choices=("local", "connected", "both"), default="both")
    parser.add_argument("--role", choices=("timing", "probe"), default="timing", help=argparse.SUPPRESS)
    parser.add_argument("--compare", type=Path, help="Compare against a compatible frozen P1-slice baseline")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args.native_dir.absolute(), args.mode, args.role), sort_keys=True))
        return
    from _support import exclusive_run
    if args.output is None or args.runs < 1:
        parser.error("--output and positive --runs are required")
    with exclusive_run(args.native_dir.parent / "runner.lock"):
        measure(args, parser)

def measure(args, parser):
    builds = {}
    for role, directory in (("timing", args.native_dir), ("probe", args.probe_native_dir)):
        metadata = json.loads((directory.parent / "build-metadata.json").read_text())
        if metadata["profile"]["debug_assertions"] or metadata["host_profile"] != "release":
            parser.error("Both native adapter and host must be Release builds")
        if metadata["instrumented"] != (role == "probe"):
            parser.error("Timing/probe builds must be separate; use test.py --release [--instrumented]")
        builds[role] = metadata
    args.output.mkdir(parents=True, exist_ok=False)
    measurements = {}
    for mode in (("local", "connected") if args.mode == "both" else (args.mode,)):
        collected = {}
        for role, directory in (("timing", args.native_dir), ("probe", args.probe_native_dir)):
            runs = []
            for index in range(args.runs):
                process = subprocess.run([sys.executable, "-I", str(Path(__file__).absolute()), "--worker",
                    "--mode", mode, "--role", role, "--native-dir", str(directory.absolute())], cwd="/tmp",
                    env={k: v for k, v in os.environ.items() if k not in {"PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"}},
                    capture_output=True, text=True)
                name = f"{mode}-{role}-{index}"
                (args.output / f"{name}.json").write_text(process.stdout)
                (args.output / f"{name}.stderr").write_text(process.stderr)
                process.check_returncode()
                run = json.loads(process.stdout)
                if run["client_sha256"] != client_hash() or run["measurement_sha256"] != hashlib.sha256(Path(__file__).read_bytes()).hexdigest():
                    raise RuntimeError("Source changed during measurement")
                if run["extension_sha256"] != builds[role]["artifact_sha256"] or run["host_sha256"] != builds[role]["host_sha256"]:
                    raise RuntimeError("Artifact changed during measurement")
                runs.append(run)
            collected[role] = runs
        summary = summarize(collected["timing"])
        for case in CASES:
            summary[case]["probes"] = [r["cases"][case]["probe"] for r in collected["probe"]]
        measurements[mode] = summary
    workspace = dict(base_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        dirty_files=subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).splitlines())
    report = dict(schema=2, status="p1_slice", workspace=workspace, builds=builds, client_sha256=client_hash(),
        measurement_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), python=sys.version,
        platform=platform.platform(), runs=args.runs, samples=SAMPLES, warmup=WARMUP,
        measurements=measurements, pending_workloads=PENDING,
        probe_scope=dict(heap="All Rust threads in the extension/host; requested live Layout bytes, excluding Python/C heaps, allocator overhead and realloc transient storage",
            copy="Explicit native RGBA input/PyBytes output, Vec-to-Arc image preparation and selected Runtime mesh snapshot arrays; implicit domain clones/realloc copies are outside this counter",
            connected="Private CPU fixture, asynchronous completion; empty diagnostic control window retained; not an Editor latency measurement"),
        thresholds=dict(p50_ratio=1.5, p95_ratio=2.0, max_process_p50_range=0.5,
            copy_growth_bytes=0, wire_poll_codec_allowance=3, max_rust_peak_growth_ratio=1.5),
        limitations=["Copy count is scoped, not total process memcpy", "Rust peak is requested live bytes, not RSS", "CPU sampling is a later P5 workload"])
    noisy = [f"{mode}.{case}" for mode, cases in measurements.items() for case, value in cases.items() if value["relative_p50_range"] > .5]
    report["thresholds"]["frozen"] = args.runs >= 5 and not noisy
    report["noise_rejections"] = noisy
    if args.compare:
        report["comparison"] = compare(report, json.loads(args.compare.read_text()))
    path = args.output / "summary.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(path)
    if args.compare and report["comparison"]:
        raise SystemExit("P0 regression gate failed: " + ", ".join(report["comparison"]))

def compare(current, baseline):
    if baseline.get("schema") != 2 or not baseline["thresholds"].get("frozen"):
        raise ValueError("Comparison requires a frozen schema-2 baseline")
    for key in ("measurement_sha256", "platform", "python", "samples", "warmup"):
        if current[key] != baseline[key]:
            raise ValueError(f"Incompatible baseline: {key}")
    failures = []
    policy = baseline["thresholds"]
    for mode, cases in current["measurements"].items():
        for case, value in cases.items():
            old = baseline["measurements"][mode][case]
            for percentile in ("p50", "p95"):
                if value[percentile + "_ns"] > old[percentile + "_ns"] * policy[percentile + "_ratio"]:
                    failures.append(f"{mode}.{case}.{percentile}")
            if value["relative_p50_range"] > policy["max_process_p50_range"]:
                failures.append(f"{mode}.{case}.noise")
            for name in {key for probe in value["probes"] for key in probe["calls"]}:
                count = max(probe["calls"].get(name, 0) for probe in value["probes"])
                old_count = max(p["calls"].get(name, 0) for p in old["probes"])
                allowance = 2 if mode == "connected" and name.startswith("python_json.") else 0
                if count > old_count + allowance:
                    failures.append(f"{mode}.{case}.{name}")
            for metric in ("copied_payload_bytes", "materialized_input_bytes", "json_encodes", "json_decodes", "json_value_builds", "json_admission_encodes"):
                allowance = policy["wire_poll_codec_allowance"] if mode == "connected" and metric in {"json_encodes", "json_decodes", "json_value_builds"} else 0
                if max(p["rust"][metric] for p in value["probes"]) > max(p["rust"][metric] for p in old["probes"]) + allowance:
                    failures.append(f"{mode}.{case}.{metric}")
            peak = lambda probes: max(p["rust"]["rust_peak_bytes"] - p["rust"]["rust_start_bytes"] for p in probes)
            if peak(value["probes"]) > max(1, peak(old["probes"])) * policy["max_rust_peak_growth_ratio"]:
                failures.append(f"{mode}.{case}.rust_peak")
    return failures

if __name__ == "__main__":
    main()
