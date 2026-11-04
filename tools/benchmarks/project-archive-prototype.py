"""Run isolated P0 directory/ZIP comparisons, with per-process RSS on POSIX.

Build first:
  cargo build --locked --release -p zipzi-project --features archive-prototype \
      --example project_archive_prototype
Run from the repository root:
  uv run --locked python tools/benchmarks/project-archive-prototype.py \
      --output target/archive-p0 --sample real=/absolute/directory-project

Five deterministic synthetic cases are always included. Extra samples are read
only; all generated files live below a new output directory. No production
project-format conversion or historical migration is performed.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
from time import perf_counter


def run(command: list[str], log_directory: Path) -> tuple[dict, dict]:
    """wait4 gives this child's high-water RSS, not cumulative child maxima."""
    start = perf_counter()
    usage = None
    with tempfile.TemporaryFile(dir=log_directory) as stdout, tempfile.TemporaryFile(
        dir=log_directory
    ) as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        if hasattr(os, "wait4"):
            _, wait_status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(wait_status)
        else:
            process.wait()
        stdout.seek(0)
        stderr.seek(0)
        output = stdout.read().decode("utf-8")
        errors = stderr.read().decode("utf-8", errors="replace")
        if process.returncode:
            raise RuntimeError(f"Command failed ({process.returncode}): {command!r}\n{errors}\n{output}")
    report = json.loads(output) if output.strip() else {}
    # macOS returns bytes, Linux returns KiB. Windows has no wait4 here.
    memory_scale = 1 if sys.platform == "darwin" else 1024
    metrics = {
        "wall_seconds": perf_counter() - start,
        "peak_rss_bytes": usage.ru_maxrss * memory_scale if usage else None,
        "input_block_operations": usage.ru_inblock if usage else None,
        "output_block_operations": usage.ru_oublock if usage else None,
        "scope": "entire child including setup and roundtrip verification",
    }
    return report, metrics


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/project_archive_prototype"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sample", action="append", default=[], metavar="LABEL=DIRECTORY")
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    samples = []
    labels = {"empty", "small-images", "large-texture", "heavy-keyforms", "missing"}
    for value in args.sample:
        label, separator, path = value.partition("=")
        if not separator or not re.fullmatch(r"[a-zA-Z0-9_-]+", label) or label in labels:
            parser.error(f"Invalid or duplicate sample label: {value}")
        labels.add(label)
        samples.append((label, Path(path).resolve(strict=True)))
    output = args.output.resolve()
    output.mkdir()  # Refuse to overwrite previous results or user artifacts.
    fixtures = output / "fixtures"
    run([str(binary), "fixtures", str(fixtures)], output)
    samples = [(label, fixtures / label) for label in (
        "empty", "small-images", "large-texture", "heavy-keyforms", "missing"
    )] + samples
    report = {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "binary": str(binary),
        "repeats": args.repeats,
        "experimental_feature": "prototype-directory-document-v21",
        "notes": [
            "Warm OS caches; no device-cache flush. Timings are local measurements, not a product SLA.",
            "Logical archive IO counts include backpatches and fingerprint reads, not physical device bytes.",
            "RSS/block counters cover the whole isolated child, including setup and untimed validation.",
            "Directory IO and temporary-disk counters are not instrumented yet; null means unmeasured.",
            "Archive resource validation decodes PNGs sequentially on open, never for save packaging.",
            "Prototype reader has no central-directory admission before ZipArchive allocation; trusted local inputs only.",
        ],
        "samples": [],
    }
    for label, directory in samples:
        print(f"Measuring {label}", file=sys.stderr, flush=True)
        archive = output / f"{label}.zipzi"
        preparation, usage = run([str(binary), "pack", str(directory), str(archive)], output)
        record = {
            "label": label,
            "kind": "synthetic" if directory.parent == fixtures else "real",
            "source": str(directory),
            "input_manifest_bytes": (directory / "project.zipzi.json").stat().st_size,
            "archive_bytes": archive.stat().st_size,
            "preparation": preparation,
            "preparation_process": usage,
            "runs": [],
        }
        for format_name, source in (("directory", directory), ("archive", archive)):
            for operation in ("open", "save", "save-as"):
                destination = output / f"{label}-{format_name}-{operation}"
                result, process_metrics = run([
                    str(binary), "bench", format_name, operation, str(source), str(destination), str(args.repeats)
                ], output)
                result["process"] = process_metrics
                record["runs"].append(result)
        report["samples"].append(record)
        (output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(output / "report.json")


if __name__ == "__main__":
    main()
