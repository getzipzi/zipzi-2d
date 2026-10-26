#!/usr/bin/env python3
"""Diagnose P0 environment drift with counterbalanced, prebuilt Python2 pairs.

Each side is a frozen tree containing client/, tools/baseline.py and separate
timing/ and probe/ native adapters, hosts and metadata. This uses the unchanged
P0 workload, samples, probes and policy. It preserves the historical comparison;
a contemporaneous control does not overwrite an earlier failed measurement.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pair_order(index):
    return ("baseline", "candidate") if index % 2 == 0 else ("candidate", "baseline")


def summarize(module, runs, policy, builds):
    measurements = {}
    for mode in ("local", "connected"):
        measurements[mode] = module.summarize(runs[mode]["timing"])
        for case in module.CASES:
            probes = [row["cases"][case]["probe"] for row in runs[mode]["probe"]]
            for probe in probes:
                module.validate_probe(mode, probe)
            measurements[mode][case]["probes"] = probes
    sample = runs["local"]["timing"][0]
    noisy = [f"{mode}.{case}" for mode, cases in measurements.items() for case, value in cases.items()
             if value["relative_p50_range"] > policy["thresholds"]["max_process_p50_range"]]
    thresholds = dict(policy["thresholds"], frozen=not noisy)
    return {"schema": 2, "status": "paired_control", "builds": builds,
        "client_sha256": sample["client_sha256"], "measurement_sha256": sample["measurement_sha256"],
        "platform": sample["platform"], "python": sample["python"], "samples": module.SAMPLES,
        "warmup": module.WARMUP, "runs": len(runs["local"]["timing"]), "measurements": measurements,
        "thresholds": thresholds, "noise_rejections": noisy}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--policy", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--pairs", type=int, default=5)
    parser.add_argument("--migration", type=Path, help="Explicit frozen API-only migration manifest; both sides must use one identical worker")
    args = parser.parse_args()
    if args.pairs < 5:
        parser.error("At least five independent pairs are required")
    policy = json.loads(args.policy.read_text())
    if policy["schema"] != 2 or not policy["thresholds"]["frozen"]:
        parser.error("Use the original frozen P0 policy")
    spec = importlib.util.spec_from_file_location("paired_p0", ROOT / "crates/zipzi-python2/tools/baseline.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    directories = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    migration=None
    if args.migration:
        from python2_p0_migration import validate_migration
        migration=validate_migration(args.migration.resolve(),args.policy.resolve(),directories)
    measurement_sha=policy['measurement_sha256'] if migration is None else migration['common_workload_sha256']
    builds = {}
    for side, directory in directories.items():
        if sha(directory / "tools/baseline.py") != measurement_sha:
            raise ValueError("Paired workloads must match the original frozen script")
        builds[side] = {}
        for role in ("timing", "probe"):
            build = json.loads((directory / role / "build-metadata.json").read_text())
            libraries = list((directory / role / "native/zipzi_native").glob("_native*.so"))
            if (len(libraries) != 1 or sha(libraries[0]) != build["artifact_sha256"]
                    or sha(directory / role / "local_host_fixture") != build["host_sha256"]
                    or build["profile"]["debug_assertions"] or build["host_profile"] != "release"
                    or build["instrumented"] != (role == "probe")):
                raise ValueError("Both sides require copied, paired Release artifacts")
            builds[side][role] = build
    args.output.mkdir(parents=True, exist_ok=False)
    runs = {side: {mode: {role: [] for role in ("timing", "probe")} for mode in ("local", "connected")}
            for side in directories}
    events = []
    # All latency runs precede instrumentation. Never compile during pairing.
    for role in ("timing", "probe"):
        for mode in ("local", "connected"):
            for index in range(args.pairs):
                for side in pair_order(index):
                    directory = directories[side]
                    name = f"{mode}-{role}-{index}-{side}"
                    result = subprocess.run([sys.executable, "-I", str(directory / "tools/baseline.py"),
                        "--worker", "--mode", mode, "--role", role, "--native-dir", str(directory / role / "native")],
                        cwd="/tmp", capture_output=True, text=True,
                        env={k: v for k, v in os.environ.items() if k not in {"PYTHONPATH", "PYTHONHOME", "ZIPZI_DEV_SESSION_DIR"}})
                    (args.output / (name + ".json")).write_text(result.stdout)
                    (args.output / (name + ".stderr")).write_text(result.stderr)
                    events.append({"side": side, "role": role, "mode": mode, "pair": index,
                                   "order": pair_order(index), "exit_code": result.returncode, "raw": name + ".json"})
                    (args.output / "runs.json").write_text(json.dumps(events, indent=2) + "\n")
                    result.check_returncode()
                    row = json.loads(result.stdout)
                    if row["extension_sha256"] != builds[side][role]["artifact_sha256"] or row["host_sha256"] != builds[side][role]["host_sha256"]:
                        raise ValueError("Artifact identity changed during measurement")
                    runs[side][mode][role].append(row)
    summaries = {side: summarize(module, value, policy, builds[side]) for side, value in runs.items()}
    control, candidate = summaries["baseline"], summaries["candidate"]
    if control["thresholds"]["frozen"]:
        failures = module.compare(candidate, control)
    else:
        failures = ["control_noise:" + name for name in control["noise_rejections"]]
    report = {"schema": 1, "status": "passed" if not failures else "failed", "pairs": args.pairs,
        "policy_sha256": sha(args.policy), "runner_sha256": sha(Path(__file__)),
        "historical_comparison": "retained separately; no historical result or threshold is overwritten",
        "order": [pair_order(i) for i in range(args.pairs)], "baseline": control, "candidate": candidate,
        "comparison": failures, "runs": events, "api_migration": migration}
    (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(args.output / "summary.json")
    if failures:
        raise SystemExit("Paired P0 gate failed: " + ", ".join(failures))


if __name__ == "__main__":
    main()
