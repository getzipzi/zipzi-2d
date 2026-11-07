#!/usr/bin/env python3
"""Run paired Release probes on the same local Mao Model3 tree (macOS)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess


def source_identity(model):
    records = []
    for path in sorted(model.parent.rglob("*")):
        if path.is_file() and not path.name.startswith("."):
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            records.append((path.relative_to(model.parent).as_posix(), path.stat().st_size, digest))
    return dict(entry=model.name, files=records,
                sha256=hashlib.sha256(json.dumps(records, separators=(",", ":")).encode()).hexdigest())


def binary_identity(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def command_output(*command):
    return subprocess.check_output(command, text=True).strip()


def run(binary, model, output, mode):
    output.mkdir()
    scratch = output / "scratch"
    scratch.mkdir()
    environment = dict(os.environ, TMPDIR=str(scratch))
    for name in ("ZIPZI_DEV_SESSION_DIR", "ZIPZI_RUNTIME_OWNER_FD"):
        environment.pop(name, None)
    with (output / "samples.jsonl").open("w") as out, (output / "time.log").open("w") as err:
        subprocess.run(["/usr/bin/time", "-l", str(binary), str(model), str(output), mode],
                       stdout=out, stderr=err, env=environment, check=True)
    records = [json.loads(line) for line in (output / "samples.jsonl").read_text().splitlines()]
    assert records[-1]["complete"] is True
    time_log = (output / "time.log").read_text()
    def counter(name):
        return int(re.search(r"^\s*(\d+)\s+" + re.escape(name) + r"$", time_log, re.M)[1])

    stage_rows = {}
    for row in records:
        if "stage" in row:
            stage_rows.setdefault(row["stage"], []).append(row)
    return {
        "rss_bytes": counter("maximum resident set size"),
        "footprint_bytes": counter("peak memory footprint"),
        "initial_signature": next(row for row in records if "mesh" in row),
        "result": records[-1],
        "stages": stage_rows,
    }


def metrics(values):
    return {"median": statistics.median(values), "min": min(values), "max": max(values)}


def summarize(workflows):
    summary = {}
    for side, rows in workflows.items():
        stages = rows[0]["stages"]
        summary[side] = {
            "rss_bytes": metrics([row["rss_bytes"] for row in rows]),
            "footprint_bytes": metrics([row["footprint_bytes"] for row in rows]),
            "stage": {
                stage: {
                    "elapsed_ns_per_step": metrics([entry["elapsed_ns"] for run in rows for entry in run["stages"][stage]]),
                    "rust_live_after": metrics([entry["rust_live_after"] for run in rows for entry in run["stages"][stage]]),
                    "rust_peak": metrics([max(entry["rust_peak"] for entry in run["stages"][stage]) for run in rows]),
                }
                for stage in stages
            },
            "rust_peak_in_measured_stages": metrics([max(entry["rust_peak"] for entries in row["stages"].values()
                                                       for entry in entries) for row in rows]),
            "rust_live_with_history": metrics([row["result"]["rust_live_with_history"] for row in rows]),
            "archive_bytes": metrics([row["result"]["archive_bytes"] for row in rows]),
        }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--baseline-commit", required=True)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--pairs", type=int, default=6)
    args = parser.parse_args()
    if args.pairs < 2 or args.pairs % 2:
        parser.error("--pairs must be an even integer of at least 2")
    if platform.system() != "Darwin":
        parser.error("The process counters use macOS /usr/bin/time -l")
    model = args.model.resolve(strict=True)
    identity = source_identity(model)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binaries = {"baseline": args.baseline.resolve(strict=True),
                "candidate": args.candidate.resolve(strict=True)}
    warmups = {side: run(binary, model, output / f"warmup-{side}", "workflow")
               for side, binary in binaries.items()}
    workflows = {side: [] for side in binaries}
    for index in range(args.pairs):
        order = tuple(binaries) if index % 2 == 0 else tuple(reversed(binaries))
        for side in order:
            workflows[side].append(run(binaries[side], model, output / f"{index + 1}-{side}", "workflow"))
    history = {}
    for side, binary in binaries.items():
        history[side] = run(binary, model, output / f"history-{side}", "history")
    baseline = workflows["baseline"][0]
    expected_stages = {key: len(rows) for key, rows in baseline["stages"].items()}
    for row in [*workflows["baseline"], *workflows["candidate"], *history.values()]:
        for key in ("signature", "mesh", "vertex", "initial_position"):
            assert row["initial_signature"][key] == baseline["initial_signature"][key], key
    for row in [*workflows["baseline"], *workflows["candidate"]]:
        assert {key: len(rows) for key, rows in row["stages"].items()} == expected_stages
        assert row["result"]["signature"] == baseline["result"]["signature"]
        assert row["result"]["usable_undo"] == row["result"]["usable_redo"] == 3
    assert source_identity(model) == identity
    with (output / "results.json").open("x") as stream:
        json.dump(dict(
            commits=dict(baseline=args.baseline_commit, candidate=args.candidate_commit),
            source=identity, binaries={side: binary_identity(path) for side, path in binaries.items()},
            pairs=args.pairs, platform=platform.platform(),
            rustc=command_output("rustc", "--version"),
            hardware=command_output("sysctl", "-n", "machdep.cpu.brand_string"),
            physical_memory=int(command_output("sysctl", "-n", "hw.memsize")),
            summary=summarize(workflows), workflows=workflows, history=history, warmups=warmups,
        ), stream, indent=2)
    print(output / "results.json")


if __name__ == "__main__":
    main()
