#!/usr/bin/env python3
"""Build/run the R0 Release CPU probe; retain raw samples and paired provenance.

Five independent baseline processes per fixture. R2 still needs alternating
baseline/candidate pairs and native window latency; this does not replace them.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def command(args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def provenance(root):
    tracked = command(["git", "ls-files", "-z"], root).split("\0")
    untracked = command(["git", "ls-files", "--others", "--exclude-standard", "-z"], root).split("\0")
    sources = {name: digest(root / name) for name in sorted(set(tracked + untracked))
               if name and (root / name).is_file() and Path(name).suffix in {".rs", ".toml", ".lock", ".py"}}
    return dict(root=str(root), revision=command(["git", "rev-parse", "HEAD"], root),
                branch=command(["git", "branch", "--show-current"], root),
                status=command(["git", "status", "--short"], root), source_sha256=sources)


CASES = {"snapshot_capture": 32, "candidate_begin": 32, "motion_plan": 32,
         "metadata_commit": 32, "undo": 32, "motion_seek_1s": 32,
         "save_first": 1, "save_warm": 8}


def summarize(records):
    if not records or records[0].get("type") != "metadata":
        raise ValueError("missing metadata")
    if records[-1].get("type") != "complete" or records[-1].get("validated") is not True:
        raise ValueError("missing successful validation")
    metadata = records[0]
    if metadata.get("debug_assertions") is not False or metadata.get("samples") != 32 or metadata.get("warmup") != 4:
        raise ValueError("expected the Release probe and its declared sample configuration")
    groups = defaultdict(list)
    seen = set()
    for row in records[1:-1]:
        if row.get("type") != "sample" or row["case"] not in CASES or row["wall_ns"] <= 0:
            raise ValueError("invalid sample")
        key = row["case"], row["index"]
        limit = 1 if row["case"] == "save_first" else CASES[row["case"]] + 4
        if row["case"] == "save_first":
            if row["index"] != 4 or row["warmup"]:
                raise ValueError("invalid first save sample")
        elif not 0 <= row["index"] < limit or row["warmup"] != (row["index"] < 4):
            raise ValueError("invalid sample index/warmup")
        if key in seen:
            raise ValueError("duplicate sample")
        seen.add(key)
        if not row["warmup"]:
            groups[row["case"]].append(row)
    if set(groups) != set(CASES):
        raise ValueError("missing benchmark cases")
    if len(seen) != sum(CASES.values()) + 4 * (len(CASES) - 1):
        raise ValueError("missing warmup samples")
    result = {}
    for case, samples in groups.items():
        if len(samples) != CASES[case]:
            raise ValueError(f"wrong sample count for {case}")
        values = sorted(row["wall_ns"] / 1e6 for row in samples)
        result[case] = dict(samples=len(values), p50_ms=statistics.median(values),
            p95_ms=values[math.ceil(.95 * len(values)) - 1],
            peak_bytes=max(row["peak_bytes"] for row in samples),
            retained_bytes=max(row["retained_bytes"] for row in samples))
    return dict(metadata=records[0], measurements=result)


def build_activity():
    lines = command(["ps", "-axo", "pid,comm"]).splitlines()
    return [line.strip() for line in lines if Path(line.split(maxsplit=1)[-1]).name in {"cargo", "rustc", "rustdoc"}]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--model", type=Path, help="optional licensed model3; never modified")
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("runs must be positive")
    output = args.output.resolve()
    if output.exists():
        parser.error("output must be fresh")
    if args.model:
        args.model = args.model.resolve(strict=True)
    output.mkdir(parents=True)
    environment = dict(date_utc=datetime.now(timezone.utc).isoformat(), platform=platform.platform(),
        rustc=command(["rustc", "-Vv"]), python=platform.python_version(),
        editor=provenance(ROOT.parent / "kasane-editor"), sdk=provenance(ROOT))
    if platform.system() == "Darwin":
        environment["hardware"] = command(["sysctl", "hw.model", "machdep.cpu.brand_string", "hw.memsize"])
        environment["graphics"] = command(["system_profiler", "SPDisplaysDataType"])
    if args.model:
        environment["model_files_sha256"] = {str(p.relative_to(args.model.parent)): digest(p)
            for p in sorted(args.model.parent.rglob("*")) if p.is_file() and p.name != ".kasane.lock"}
    (output / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    build = subprocess.run(["cargo", "build", "--locked", "--release", "-p", "kasane-sdk",
        "--example", "runtime_baseline", "--message-format=json-render-diagnostics"], cwd=ROOT, capture_output=True, text=True)
    (output / "build.jsonl").write_text(build.stdout)
    (output / "build.log").write_text(build.stderr)
    build.check_returncode()
    executables = {row["executable"] for line in build.stdout.splitlines() if (row := json.loads(line)).get("reason") == "compiler-artifact"
                   and row.get("target", {}).get("name") == "runtime_baseline" and row.get("executable")}
    if len(executables) != 1:
        raise RuntimeError("expected one Cargo executable")
    executable = executables.pop()
    summary = {}
    for fixture in ["synthetic"] + (["model"] if args.model else []):
        runs = []
        for index in range(args.runs):
            name = f"{fixture}-{index}"
            invocation = [executable, str(output / f"{name}-project")]
            if fixture == "model":
                invocation.append(str(args.model))
            if platform.system() == "Darwin":
                invocation = ["/usr/bin/time", "-l", *invocation]
            if activity := build_activity():
                raise RuntimeError(f"concurrent build detected before timing; retry with fresh output: {activity}")
            process = subprocess.run(invocation, cwd=ROOT, capture_output=True, text=True,
                env={key: value for key, value in os.environ.items() if key != "KASANE_DEV_SESSION_DIR"})
            (output / f"{name}.jsonl").write_text(process.stdout)
            (output / f"{name}.stderr").write_text(process.stderr)
            process.check_returncode()
            if activity := build_activity():
                raise RuntimeError(f"concurrent build detected after timing; discard timing: {activity}")
            run = summarize([json.loads(line) for line in process.stdout.splitlines()])
            runs.append(run)
        summary[fixture] = dict(runs=runs, aggregate={case: dict(
            median_p50_ms=statistics.median(run["measurements"][case]["p50_ms"] for run in runs),
            median_p95_ms=statistics.median(run["measurements"][case]["p95_ms"] for run in runs),
            p50_range_ms=[min(run["measurements"][case]["p50_ms"] for run in runs), max(run["measurements"][case]["p50_ms"] for run in runs)],
            max_peak_bytes=max(run["measurements"][case]["peak_bytes"] for run in runs),
            max_retained_bytes=max(run["measurements"][case]["retained_bytes"] for run in runs),
        ) for case in CASES})
    for label, root in [("editor", ROOT.parent / "kasane-editor"), ("sdk", ROOT)]:
        if provenance(root) != environment[label]:
            raise RuntimeError(f"{label} sources changed during measurement; discard timing")
    if args.model:
        after = {str(p.relative_to(args.model.parent)): digest(p)
            for p in sorted(args.model.parent.rglob("*")) if p.is_file() and p.name != ".kasane.lock"}
        if after != environment["model_files_sha256"]:
            raise RuntimeError("model files changed during measurement; discard timing")
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(output / "summary.json")


if __name__ == "__main__":
    main()
