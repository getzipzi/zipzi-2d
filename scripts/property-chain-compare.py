#!/usr/bin/env python3
"""Rerun saved before/after plain binaries in alternating order with identical workloads."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import gzip
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time

spec = importlib.util.spec_from_file_location("chain", Path(__file__).with_name("property-chain-benchmark.py"))
chain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(chain)


def compatible(before, after):
    for field in ("model", "samples", "warmup", "updates", "filter", "reader_policy", "rounds"):
        if before["arguments"][field] != after["arguments"][field]:
            raise ValueError(f"different workload argument: {field}")
    harness = "kasane-editor/crates/editor-core/examples/property_chain_benchmark.rs"
    if before["sources"][harness] != after["sources"][harness]:
        raise ValueError("benchmark workload source changed")
    if before["model_files"] != after["model_files"]:
        raise ValueError("model changed")
    if before["rustc"] != after["rustc"]:
        raise ValueError("different build configuration: rustc")
    # A separate output directory is useful for isolated builds and does not
    # change optimization or the compilation target.
    environment = lambda run: {
        key: value for key, value in run["build_environment"].items()
        if key != "CARGO_TARGET_DIR"
    }
    if environment(before) != environment(after):
        raise ValueError("different build configuration: build_environment")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("output", type=Path)
    options = parser.parse_args()
    directories = {"before": options.before.resolve(strict=True), "after": options.after.resolve(strict=True)}
    provenance = {mode: json.loads((directory / "provenance.json").read_text()) for mode, directory in directories.items()}
    compatible(provenance["before"], provenance["after"])
    args = provenance["after"]["arguments"]
    model = Path(args["model"])
    if chain.model_manifest(model) != provenance["after"]["model_files"]:
        raise ValueError("model files no longer match saved builds")
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binaries = {}
    for mode, directory in directories.items():
        binary = directory / "property-chain-plain"
        if chain.digest(binary) != provenance[mode]["binaries"]["plain"]:
            raise ValueError(f"{mode} binary hash mismatch")
        binaries[mode] = output / f"property-chain-{mode}"
        shutil.copy2(binary, binaries[mode])
    sources = chain.source_manifest()
    record = {
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "provenance": {mode: str(directory / "provenance.json") for mode, directory in directories.items()},
        "binaries": {mode: chain.digest(binary) for mode, binary in binaries.items()},
        "workload": args, "sources_at_measurement": sources,
    }
    (output / "provenance.json").write_text(json.dumps(record, indent=2) + "\n")
    env = os.environ.copy()
    env.pop("KASANE_DEV_SESSION_DIR", None)
    runs = []
    for index in range(args["rounds"]):
        for mode in (["before", "after"] if index % 2 == 0 else ["after", "before"]):
            if activity := chain.build_activity():
                raise RuntimeError(f"concurrent compiler detected: {activity}")
            raw = output / f"round-{index+1}-{mode}.jsonl"
            cmd = [str(binaries[mode]), str(model), str(args["samples"]), str(args["warmup"]), str(args["updates"]), args["filter"], args["reader_policy"]]
            print(f"Round {index+1}/{args['rounds']}: {mode}", flush=True)
            thermal = chain.thermal()
            with raw.open("wb") as stdout, raw.with_suffix(".stderr").open("wb") as stderr:
                subprocess.run(cmd, cwd=chain.ROOT, env=env, stdout=stdout, stderr=stderr, check=True)
            if chain.build_activity() or chain.source_manifest() != sources:
                raise RuntimeError("compiler activity or source changes during measurement")
            result = chain.summarize([json.loads(line) for line in raw.read_text().splitlines()])
            if result["metadata"]["instrumented"]:
                raise ValueError("plain binary required")
            if runs:
                for field in ("meshes", "vertices", "triangles", "parameters", "motions", "expressions", "motion_tracks", "motion_keys", "samples", "warmup", "updates", "preview_mode", "retained_previous_index"):
                    if result["metadata"][field] != runs[0]["metadata"][field]:
                        raise ValueError(f"metadata mismatch: {field}")
                if set(result["measurements"]) != set(runs[0]["measurements"]):
                    raise ValueError("workload coverage changed")
            result.update(round=index+1, mode=mode, raw=raw.name+".gz", thermal_before=thermal, thermal_after=chain.thermal())
            runs.append(result)
            with raw.open("rb") as source, gzip.open(str(raw)+".gz", "wb") as destination:
                shutil.copyfileobj(source, destination)
            raw.unlink()
            time.sleep(1)
    if chain.model_manifest(model) != provenance["after"]["model_files"]:
        raise ValueError("model changed during measurement")
    measurements = {}
    for key in sorted(runs[0]["measurements"]):
        values = {mode: [r["measurements"][key]["median_us"] for r in runs if r["mode"] == mode] for mode in directories}
        ratios = [next(r["measurements"][key]["median_us"] for r in runs if r["round"] == i+1 and r["mode"] == "before") / next(r["measurements"][key]["median_us"] for r in runs if r["round"] == i+1 and r["mode"] == "after") for i in range(args["rounds"])]
        measurements[key] = {"before_us": statistics.median(values["before"]), "after_us": statistics.median(values["after"]), "before_runs_us": values["before"], "after_runs_us": values["after"], "paired_speedups": ratios, "paired_speedup_median": statistics.median(ratios)}
    (output / "summary.json").write_text(json.dumps({"schema_version": 1, "runs": runs, "comparisons": measurements}, indent=2) + "\n")
    lines = ["# Alternating before/after plain binaries", "", f"Identical saved benchmark source, model, workloads and recorded build configuration. {args['rounds']} fresh processes per version.", "", "| Case / phase | Before μs | After μs | Median paired speedup |", "| --- | ---: | ---: | ---: |"]
    for key, value in measurements.items():
        lines.append(f"| {key} | {value['before_us']:.2f} | {value['after_us']:.2f} | {value['paired_speedup_median']:.2f}× |")
    (output / "summary.md").write_text("\n".join(lines) + "\n")
    print(output / "summary.md")


if __name__ == "__main__":
    main()
