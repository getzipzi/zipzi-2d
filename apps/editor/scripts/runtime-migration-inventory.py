#!/usr/bin/env python3
"""Index every session/publication/IO/lifecycle reference before R2 migration.

This deliberately includes reads, comments and test code so a source search
cannot silently omit a write. The domain checklist explains the actual boundary.
It is a review index, not proof that migration is complete.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/plans/sdk-runtime/editor-migration.json"
REFERENCE = re.compile(
    r"\bsession\b|session_mut\s*\(|AuthoringSession|drain_events|commit_to|begin_owned_edit|"
    r"(?:prepare|finish)_(?:load|save)|project_identity|ensure_available_path|find_path\s*\(|"
    r"on_window_closed|on_window_should_close|\bcx\.quit\s*\("
)


def classify(path: str) -> tuple[str, str]:
    if "/headless_tests/" in path or "/tests/" in path or path.endswith("/test_scene.rs"):
        return "test-fixture", "retain baseline; add Runtime-backed fixtures in R1/R2"
    if path.endswith("/dev_session.rs") or path.endswith("/benchmark.rs"):
        return "host-adapter", "use common Runtime/host entrypoints; retain dev and benchmark behavior"
    if path == "crates/editor/src/project.rs":
        return "host-task-completion", "move task owner and completion before weak view update to application Host"
    if path == "crates/editor/src/lib.rs":
        return "host-lifecycle", "close detaches view; explicit shutdown owns quit and dirty decisions"
    if "/workspace/io.rs" in path or "/workspace/entry.rs" in path:
        return "runtime-registry-io", "Runtime owns formal session, Busy job, paths and reservations; view retains frozen projection"
    if "/animation/" in path:
        return "view-animation", "view Preview/playback stay local; timeline formal writes use Runtime edit/publication"
    if path.endswith("/session.rs") or "/workspace/" in path:
        return "view-coordination", "DocumentSession view/tool/selection stay local; formal document references Runtime SessionId"
    return "sdk-document-adapter", "formal writes/candidates/history use typed Runtime; projections, caches and selection stay view-owned"


def collect(root: Path = ROOT) -> list[dict]:
    rows = []
    for directory in ["crates/editor-core/src", "crates/editor/src", "crates/editor/headless_tests"]:
        for path in sorted((root / directory).rglob("*.rs")):
            relative = str(path.relative_to(root))
            area, destination = classify(relative)
            function = None
            for line, text in enumerate(path.read_text().splitlines(), 1):
                if found := re.search(r"\bfn\s+(\w+)", text):
                    function = found[1]
                if REFERENCE.search(text):
                    rows.append(dict(file=relative, line=line, enclosing_function_hint=function,
                        text=text.strip(), area=area, destination=destination, stage="R2", status="not-migrated"))
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    rows = collect()
    encoded = "[\n" + ",\n".join("  " + json.dumps(row, ensure_ascii=False) for row in rows) + "\n]\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != encoded:
            print("Runtime migration source index changed; review and regenerate scripts/runtime-migration-inventory.py")
            return 1
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(encoded)
    print(json.dumps(dict(references=len(rows), files=len({row["file"] for row in rows}),
                          areas=Counter(row["area"] for row in rows)), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
