#!/usr/bin/env python3
"""Verify the immutable P1 import, including blobs, modes and both parents."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)

baseline = json.loads((ROOT / "docs/repository-merge/baseline.json").read_text())
record = json.loads((ROOT / "docs/repository-merge/import.json").read_text())
commit = record["commit"]
parents = git("show", "-s", "--format=%P", commit).decode().split()
assert len(parents) == 2
assert parents[1] == baseline["editor"]["sha"]
for source in ("sdk", "editor"):
    subprocess.run(["git", "merge-base", "--is-ancestor", baseline[source]["sha"], commit], cwd=ROOT, check=True)
original = git("ls-tree", "-r", "-z", baseline["editor"]["sha"])
imported = git("ls-tree", "-r", "-z", commit + ":apps/editor")
assert original == imported, "Editor path/blob/mode mismatch"
before = git("ls-tree", "-r", "-z", parents[0])
after = git("ls-tree", "-r", "-z", commit)
without_editor = b"\0".join(row for row in after.split(b"\0") if row and not row.split(b"\t", 1)[1].startswith(b"apps/editor/")) + b"\0"
assert before == without_editor, "SDK tree modified by import"
assert git("ls-tree", baseline["sdk"]["sha"], "modules/purism-core") == git("ls-tree", commit, "modules/purism-core")
print("PASS: exact Editor tree; SDK unchanged; both histories reachable; Purism gitlink unchanged")
