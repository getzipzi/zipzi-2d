#!/usr/bin/env python3
"""Negative controls for Purism's conformance runner and numeric oracle."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "modules/purism-core/scripts/run-tests.sh"


def checked(command, expected, env=None):
    result = subprocess.run(command, cwd=ROOT, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if (result.returncode == 0) != expected:
        raise AssertionError(f"Unexpected exit {result.returncode}: {command}\n{result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stageplay", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="kasane-oracle-") as tmp:
        base = Path(tmp)
        reference = base / "value.txt"
        reference.write_text("1.0\n")
        for emitted, expected in [("1.0", True), ("2.0", False),
                                  ("nan", False), ("inf", False)]:
            checked([args.stageplay, "-i", reference, "-e", f"emit {emitted}"], expected)
        for invalid in ("nan", "inf"):
            reference.write_text(invalid + "\n")
            checked([args.stageplay, "-i", reference, "-e", f"emit {invalid}"], False)
        reference.write_text("1.0\n")
        checked([args.stageplay, "-i", reference, "-e", "bad_command"], False)
        checked([args.stageplay, "-i", reference, "-e", "tolerance nan; emit 2.0"], False)
        empty = base / "empty.txt"
        empty.write_text("")
        checked([args.stageplay, "-i", empty, "-e", "emit 1.0"], False)
        checked([args.stageplay, "-i", empty, "-e", "set x 1"], False)

        data = base / "data"
        refs = base / "refs"
        data.mkdir()
        refs.mkdir()
        stub = base / "stageplay"
        stub.write_text("#!/bin/sh\nexit 0\n")
        stub.chmod(0o755)
        env = {**os.environ, "STAGEPLAY": str(stub),
               "TEST_DATA": str(data), "REF_DIR": str(refs)}
        checked(["sh", RUNNER], False, env)  # no models
        (data / "model.moc3").write_bytes(b"MOC3")
        checked(["sh", RUNNER], False, env)  # no references
        scenarios = ROOT / "modules/purism-core/scripts/scenarios"
        for scenario in scenarios.glob("*.tcl"):
            (refs / f"model_{scenario.stem}.txt").write_text("ok\n")
        checked(["sh", RUNNER], True, env)
        stub.write_text("#!/bin/sh\nexit 9\n")
        checked(["sh", RUNNER], False, env)  # child failure
        env["TEST_DATA"] = str(base / "missing")
        checked(["sh", RUNNER], False, env)
    print("Validation negative controls passed")


if __name__ == "__main__":
    main()
