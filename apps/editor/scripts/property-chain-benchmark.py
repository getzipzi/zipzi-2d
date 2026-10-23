#!/usr/bin/env python3
"""Build paired Release probes, validate every run, retain raw data and provenance."""
from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime, timezone
import gzip
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tarfile
import time

from workspace_tools import ROOT, digest, source_manifest, repository


def command(args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def probe_executable(messages):
    """Use Cargo's actual artifact path, including configured target triples."""
    executables = set()
    for line in messages.splitlines():
        artifact = json.loads(line)
        target = artifact.get("target", {})
        if (artifact.get("reason") == "compiler-artifact"
                and target.get("name") == "property_chain_benchmark"
                and "example" in target.get("kind", [])
                and artifact.get("executable")):
            executables.add(Path(artifact["executable"]))
    if len(executables) != 1:
        raise RuntimeError("Cargo did not report exactly one property-chain benchmark executable")
    return executables.pop()


def percentile(values, fraction):
    """Nearest-rank empirical quantile, with no interpolation or tail extrapolation."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def validate(records):
    if not records or records[0].get("type") != "metadata":
        raise ValueError("missing run metadata")
    if records[-1] != {"type": "complete", "validated": True}:
        raise ValueError("incomplete or unvalidated benchmark run")
    metadata = records[0]
    groups = defaultdict(list)
    identities = set()
    for row in records[1:-1]:
        if row["type"] == "skip":
            continue
        if row["type"] != "sample":
            raise ValueError("unexpected record")
        identity = (row["case"], row["phase"], row["sample"], row["update"])
        if identity in identities:
            raise ValueError(f"duplicate sample: {identity}")
        identities.add(identity)
        if not 0 <= row["sample"] < metadata["samples"] or row["wall_ns"] <= 0:
            raise ValueError("invalid sample number or timing")
        if row["phase"] in ("update", "preview"):
            if not 0 <= row["update"] < metadata["updates"]:
                raise ValueError("invalid drag update number")
        elif row["update"] != 0:
            raise ValueError("unexpected update number outside drag")
        stages = row["stages"]
        if metadata["instrumented"]:
            root = stages.get("benchmark.operation")
            if not root or root["calls"] != 1:
                raise ValueError("missing root span")
            if sum(s["self_ns"] for s in stages.values()) != root["inclusive_ns"]:
                raise ValueError("nested timing does not reconcile")
            if root["inclusive_ns"] > row["wall_ns"]:
                raise ValueError("span exceeds operation boundary")
            for stage in stages.values():
                if stage["calls"] <= 0 or not 0 <= stage["self_ns"] <= stage["inclusive_ns"]:
                    raise ValueError("invalid stage timing")
        elif stages:
            raise ValueError("unexpected probes in uninstrumented binary")
        groups[row["case"], row["phase"]].append(row)
    if not groups:
        raise ValueError("no measured workloads")
    for (case, phase), rows in groups.items():
        expected = metadata["samples"]
        if phase in ("update", "preview"):
            expected *= metadata["updates"]
        elif phase in ("finish", "after_finish", "cancel", "after_cancel"):
            even = sum(i % 2 == 0 for i in range(metadata["warmup"], metadata["warmup"] + metadata["samples"]))
            expected = even if phase in ("finish", "after_finish") else metadata["samples"] - even
        if len(rows) != expected:
            raise ValueError(f"{case}/{phase}: expected {expected} samples, got {len(rows)}")
    for case in {case for case, _ in groups}:
        expected_phases = (
            {"begin", "update", "preview", "finish", "after_finish", "cancel", "after_cancel"}
            if case.endswith("_drag") else
            {"commit", "after_commit"} if case.endswith("_commit") else {"read"}
        )
        if {phase for name, phase in groups if name == case} != expected_phases:
            raise ValueError(f"missing or unexpected phases for {case}")
    return metadata, groups


def summarize(records):
    metadata, groups = validate(records)
    # Sum paired phases from the SAME operation, never add unrelated quantiles.
    for case in {case for case, _ in groups}:
        for first, second, combined in [
            ("update", "preview", "update_and_preview"),
            ("commit", "after_commit", "commit_and_preview"),
            ("finish", "after_finish", "finish_and_preview"),
            ("cancel", "after_cancel", "cancel_and_preview"),
        ]:
            if (case, first) not in groups:
                continue
            later = {(r["sample"], r["update"]): r for r in groups[case, second]}
            paired = []
            for row in groups[case, first]:
                following = later[row["sample"], row["update"]]
                stages = {}
                for part in [row, following]:
                    for name, stage in part["stages"].items():
                        target = stages.setdefault(name, {"calls":0, "inclusive_ns":0, "self_ns":0})
                        for field in target:
                            target[field] += stage[field]
                paired.append({**row, "wall_ns":row["wall_ns"]+following["wall_ns"], "stages":stages})
            groups[case, combined] = paired
    result = {}
    for (case, phase), rows in groups.items():
        walls = [row["wall_ns"] for row in rows]
        total = sum(walls)
        stages = defaultdict(lambda: {"calls": [], "inclusive": [], "self": []})
        for row in rows:
            for name, stage in row["stages"].items():
                stages[name]["calls"].append(stage["calls"])
                stages[name]["inclusive"].append(stage["inclusive_ns"])
                stages[name]["self"].append(stage["self_ns"])
        result[f"{case}/{phase}"] = {
            "n": len(rows),
            "median_us": statistics.median(walls) / 1000,
            "p95_us": percentile(walls, 0.95) / 1000,
            "mean_us": statistics.mean(walls) / 1000,
            "max_us": max(walls) / 1000,
            "stages": {
                name: {
                    "calls_per_operation": sum(stage["calls"]) / len(rows),
                    "calls_min": min(stage["calls"] + ([0] if len(stage["calls"]) < len(rows) else [])),
                    "calls_max": max(stage["calls"]),
                    "inclusive_mean_us": sum(stage["inclusive"]) / len(rows) / 1000,
                    "self_mean_us": sum(stage["self"]) / len(rows) / 1000,
                    "self_share": sum(stage["self"]) / total,
                } for name, stage in stages.items()
            },
        }
    return {"metadata": metadata, "measurements": result}


def build_activity():
    lines = command(["ps", "-axo", "pid,comm"]).splitlines()
    return [line.strip() for line in lines if Path(line.split(maxsplit=1)[-1]).name in {"cargo", "rustc", "rustdoc"}]


def thermal():
    if platform.system() != "Darwin":
        return None
    return subprocess.run(["pmset", "-g", "therm"], capture_output=True, text=True).stdout.strip()


def model_manifest(model):
    return {
        str(path.relative_to(model.parent)): digest(path)
        for path in sorted(model.parent.rglob("*"))
        if path.is_file() and path.name != ".kasane.lock"
    }


def report_markdown(summary):
    lines = [
        "# Property editing chain benchmark", "",
        "Synchronous wall time on the calling thread. No GPU/window/FPS measurement.",
        "Uninstrumented totals are authoritative; instrumented self time attributes cost.",
        "Each entry is the median of per-process medians; bracketed range spans those medians.",
        "P95 is empirical within each process, not an established tail-latency bound.", "",
        "| Case / phase | Plain median μs [range] | Median run P95 μs | Probe/plain median ratio |",
        "| --- | ---: | ---: | ---: |",
    ]
    runs = summary["runs"]
    keys = sorted(runs[0]["measurements"])
    for key in keys:
        plain = [r["measurements"][key] for r in runs if not r["metadata"]["instrumented"]]
        traced = [r["measurements"][key] for r in runs if r["metadata"]["instrumented"]]
        medians = [r["median_us"] for r in plain]
        ratios = [
            runs[2*i + (0 if runs[2*i]["metadata"]["instrumented"] else 1)]["measurements"][key]["median_us"]
            / runs[2*i + (1 if runs[2*i]["metadata"]["instrumented"] else 0)]["measurements"][key]["median_us"]
            for i in range(len(runs)//2)
        ]
        lines.append(f"| {key} | {statistics.median(medians):.2f} [{min(medians):.2f}–{max(medians):.2f}] | {statistics.median(r['p95_us'] for r in plain):.2f} | {statistics.median(ratios):.3f} |")
    lines += ["", "## Instrumented attribution", "", "Self percentages are non-overlapping, relative to that phase's total wall time.", "Inclusive timings overlap and must not be added. Counts include nested calls.", ""]
    for key in keys:
        if key.endswith("/read"):
            continue
        traced = [r["measurements"][key] for r in runs if r["metadata"]["instrumented"]]
        stage_names = set().union(*(r["stages"] for r in traced))
        names = sorted(stage_names, key=lambda name: statistics.median(r["stages"].get(name, {}).get("self_share", 0) for r in traced), reverse=True)
        lines += [f"### {key}", "", "| Stage | Calls/op | Self μs | Self % | Inclusive μs |", "| --- | ---: | ---: | ---: | ---: |"]
        for name in names:
            stages = [r["stages"].get(name, {}) for r in traced]
            metric = lambda field: statistics.median(s.get(field, 0) for s in stages)
            lines.append(f"| {name} | {metric('calls_per_operation'):.2f} | {metric('self_mean_us'):.2f} | {metric('self_share')*100:.1f}% | {metric('inclusive_mean_us'):.2f} |")
        lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--samples", type=int, default=60, help="measured commits or drag gestures per case/process")
    parser.add_argument("--warmup", type=int, default=10)
    parser.add_argument("--updates", type=int, default=12, help="distinct alternating values per drag")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--filter", default="all")
    parser.add_argument("--reader-policy", choices=["retained", "unique"], default="retained", help="retain previous panel index across editing, or run a unique-owner control")
    options = parser.parse_args()
    if options.samples < 2 or options.warmup < 1 or options.updates < 2 or options.rounds < 1:
        parser.error("need >=2 samples/updates, >=1 warmup/round")
    model = options.model.resolve(strict=True)
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before_sources = source_manifest()
    # Include full sources as well as diffs, including new untracked probe files.
    with tarfile.open(output / "sources.tar.gz", "w:gz") as archive:
        for relative in sorted(before_sources):
            archive.add(ROOT / relative, arcname=relative, recursive=False)
    provenance = {
        "schema_version": 2, "started_utc": datetime.now(timezone.utc).isoformat(),
        "arguments": {**vars(options), "model": str(model), "output": str(output)},
        "platform": platform.platform(), "machine": platform.machine(), "invocation_cwd": str(Path.cwd()),
        "rustc": command(["rustc", "-Vv"]), "sources": before_sources,
        "build_environment": {key: value for key, value in os.environ.items() if key in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR"} or key.startswith("CARGO_PROFILE_RELEASE_")},
        "repository": repository(), "lock_sha256": digest(ROOT / "Cargo.lock"), "model_files": {},
    }
    if platform.system() == "Darwin":
        provenance["hardware"] = command(["sysctl", "hw.model", "hw.memsize", "machdep.cpu.brand_string"])
    (output / "repository.diff").write_text(command(["git", "diff", "--binary", "HEAD"]))
    # Fingerprint the model subtree, including moc3, attachments and textures.
    provenance["model_files"] = model_manifest(model)
    env = os.environ.copy()
    env.pop("KASANE_DEV_SESSION_DIR", None)
    if env.get("CARGO_TARGET_DIR"):
        env["CARGO_TARGET_DIR"] = str(Path(env["CARGO_TARGET_DIR"]).resolve())
    binaries = {}
    for mode in ["plain", "instrumented"]:
        args = ["cargo", "build", "--locked", "--release", "--message-format=json-render-diagnostics", "-p", "editor-core", "--example", "property_chain_benchmark"]
        if mode == "instrumented":
            args += ["--features", "bench-instrumentation"]
        build = subprocess.run(args, cwd=ROOT, env=env, check=True, stdout=subprocess.PIPE, text=True)
        binary = probe_executable(build.stdout)
        binaries[mode] = output / f"property-chain-{mode}"
        shutil.copy2(binary, binaries[mode])
    provenance["binaries"] = {mode: digest(path) for mode, path in binaries.items()}
    if source_manifest() != before_sources:
        raise RuntimeError("sources changed during build; rerun against stable sources")
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    results = []
    for round_index in range(options.rounds):
        for mode in (["plain", "instrumented"] if round_index % 2 == 0 else ["instrumented", "plain"]):
            if activity := build_activity():
                raise RuntimeError(f"concurrent compiler detected before timing: {activity}")
            raw = output / f"round-{round_index+1}-{mode}.jsonl"
            args = [str(binaries[mode]), str(model), str(options.samples), str(options.warmup), str(options.updates), options.filter, options.reader_policy]
            print(f"Round {round_index+1}/{options.rounds}: {mode}", flush=True)
            therm_before = thermal()
            with raw.open("wb") as stdout, raw.with_suffix(".stderr").open("wb") as stderr:
                subprocess.run(args, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, check=True)
            if activity := build_activity():
                raise RuntimeError(f"concurrent compiler detected after timing: {activity}")
            if source_manifest() != before_sources:
                raise RuntimeError("sources changed during measurement; discard this run")
            records = [json.loads(line) for line in raw.read_text().splitlines()]
            result = summarize(records)
            result.update(round=round_index+1, raw=raw.name+".gz", thermal_before=therm_before, thermal_after=thermal())
            if result["metadata"]["instrumented"] != (mode == "instrumented"):
                raise RuntimeError("binary feature mismatch")
            if results and set(result["measurements"]) != set(results[0]["measurements"]):
                raise RuntimeError("paired runs covered different workloads")
            results.append(result)
            with raw.open("rb") as source, gzip.open(str(raw)+".gz", "wb") as destination:
                shutil.copyfileobj(source, destination)
            raw.unlink()
            time.sleep(1)
    summary = {"schema_version":1, "provenance":"provenance.json", "runs":results}
    if model_manifest(model) != provenance["model_files"]:
        raise RuntimeError("model files changed during measurement; discard these results")
    (output / "summary.json").write_text(json.dumps(summary, indent=2)+"\n")
    (output / "summary.md").write_text(report_markdown(summary)+"\n")
    print(output / "summary.md")


if __name__ == "__main__":
    main()
