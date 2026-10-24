"""Compare native Metal group crops against the common-surface A/B control."""
from __future__ import annotations

import argparse
import json
import re
import statistics
import subprocess
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--out", type=Path, default=Path("target/offscreen-benchmark"))
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be positive")
    root = Path(__file__).resolve().parents[2]
    output = (root / args.out).resolve()
    output.mkdir(parents=True, exist_ok=True)
    runs: dict[str, list[dict]] = {"global": [], "cropped": []}
    for iteration in range(args.rounds):
        # Alternate execution order to reduce thermal/order bias.
        for policy in (["global", "cropped"] if iteration % 2 == 0 else ["cropped", "global"]):
            directory = output / f"{iteration}-{policy}"
            command = ["cargo", "run", "--locked", "--release", "-p", "zipzi-render-metal", "--example", "offscreen_benchmark"]
            if policy == "global":
                command += ["--features", "benchmark-global-surfaces"]
            command += ["--", str(directory)]
            result = subprocess.run(command, cwd=root, capture_output=True, text=True)
            (output / f"{iteration}-{policy}.log").write_text(result.stdout + result.stderr)
            if result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
            print(f"round={iteration} policy={policy}\n{result.stdout}", flush=True)
            parsed = {}
            for line in result.stdout.splitlines():
                match = re.match(r"^(\w+): (.*)$", line)
                if match:
                    parsed[match[1]] = {key: float(value) for key, value in re.findall(r"(\w+)=([\d.]+)", match[2])}
            if len(parsed) != 4:
                raise RuntimeError(f"Expected four benchmark cases, got {parsed}")
            runs[policy].append(parsed)
    summary = {}
    for name in runs["global"][0]:
        metrics = {
            policy: {key: statistics.median(run[name][key] for run in values) for key in values[0][name]}
            for policy, values in runs.items()
        }
        maximum = 0
        different = 0
        for iteration in range(args.rounds):
            before = (output / f"{iteration}-global" / f"{name}.rgba").read_bytes()
            after = (output / f"{iteration}-cropped" / f"{name}.rgba").read_bytes()
            if len(before) != len(after):
                raise AssertionError(f"{name}: image extent changed")
            if before != after:
                for a, b in zip(before, after):
                    maximum = max(maximum, abs(a-b))
                    different += a != b
        if maximum > 1:
            raise AssertionError(f"{name}: pixel difference {maximum} exceeds 1/255")
        summary[name] = {
            **metrics,
            "gpu_reduction_percent": 100 * (1 - metrics["cropped"]["gpu_ms"] / metrics["global"]["gpu_ms"]),
            "color_reduction_percent": 100 * (1 - metrics["cropped"]["color_bytes"] / metrics["global"]["color_bytes"]),
            "max_channel_difference": maximum,
            "different_channels": different,
        }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    print(f"Saved reports and RGBA captures to {output}")


if __name__ == "__main__":
    main()
