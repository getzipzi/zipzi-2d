#!/usr/bin/env python3
"""Inventory the v4 public Python contract and capture source test evidence."""
from __future__ import annotations

import argparse
import ast
from collections import Counter
import dataclasses
from datetime import datetime, timezone
import enum
import hashlib
import importlib
import inspect
import json
from pathlib import Path
import platform
import subprocess
import sys
import sysconfig
import tempfile
import types
import typing
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/zipzi-python"
SOURCE = PACKAGE / "client/python"
SPECIAL_METHODS = {"__enter__", "__exit__", "__init__"}


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    # Evidence is append-only at the artifact level. A new run needs a new path.
    with path.open("x") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def source_identity() -> dict:
    paths = subprocess.check_output(["git", "ls-files", "-z", "--",
        "Cargo.toml", "Cargo.lock", "crates"], cwd=ROOT).decode().split("\0")
    files = {name: sha256(ROOT / name) if (ROOT / name).is_file() else None
             for name in sorted(filter(None, paths))}
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard",
        "-z", "--", "crates"], cwd=ROOT).decode().split("\0")
    files.update({name: sha256(ROOT / name) for name in sorted(filter(None, untracked))
                  if (ROOT / name).is_file()})
    return {
        "git_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "git_status": subprocess.check_output(["git", "status", "--porcelain=v1"], cwd=ROOT, text=True),
        "files": files,
        "sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
    }


def artifact_identity(path: Path) -> dict:
    if not path.is_file():
        return {"path": str(path), "status": "missing", "sha256": None}
    return {"path": str(path), "status": "present_unverified",
            "bytes": path.stat().st_size, "sha256": sha256(path)}


def load_client() -> types.ModuleType:
    sys.path.insert(0, str(SOURCE))
    client = importlib.import_module("zipzi")
    if Path(client.__file__).resolve() != SOURCE / "zipzi/__init__.py":
        raise RuntimeError("Inventory must read this checkout's pure client")
    return client


def inspect_callable(value: object, aliases: dict[int, str]) -> dict:
    result = {}
    try:
        signature = inspect.signature(value, eval_str=False)
        def annotation(item):
            if item is inspect.Signature.empty or isinstance(item, str):
                return item
            return aliases.get(id(item), inspect.formatannotation(item))
        signature = signature.replace(parameters=[p.replace(annotation=annotation(p.annotation))
            for p in signature.parameters.values()], return_annotation=annotation(signature.return_annotation))
        result["signature"] = signature.format(quote_annotation_strings=False)
    except (TypeError, ValueError) as error:
        result["signature_error"] = f"{type(error).__name__}: {error}"
    try:
        typing.get_type_hints(value)
        result["type_hints"] = "resolved"
    except (NameError, TypeError, AttributeError) as error:
        result["type_hints"] = f"{type(error).__name__}: {error}"
    result["has_doc"] = bool(inspect.getdoc(value))
    return result


def instance_attributes(cls: type, trees: dict[str, ast.Module]) -> set[str]:
    """Read simple self assignments without constructing any live SDK object."""
    names = set()
    for base in cls.__mro__:
        if not base.__module__.startswith("zipzi"):
            continue
        source = inspect.getsourcefile(base)
        if source is None:
            continue
        if source not in trees:
            trees[source] = ast.parse(Path(source).read_text())
        declarations = [node for node in ast.walk(trees[source])
                        if isinstance(node, ast.ClassDef) and node.name == base.__name__]
        for node in (node for declaration in declarations for node in ast.walk(declaration)):
            if (isinstance(node, ast.Attribute) and isinstance(node.ctx, ast.Store)
                    and isinstance(node.value, ast.Name) and node.value.id == "self"
                    and not node.attr.startswith("_")):
                names.add(node.attr)
    return names


def inventory() -> dict:
    client = load_client()
    modules = {"zipzi": client}
    # Include directly importable public modules, even if root __all__ omits them.
    for path in sorted((SOURCE / "zipzi").glob("*.py")):
        if not path.name.startswith("_"):
            name = f"zipzi.{path.stem}"
            modules[name] = importlib.import_module(name)
    aliases = {}
    for name, module in sorted(modules.items(), key=lambda item: -len(item[0])):
        for key, value in vars(module).items():
            if not key.startswith("_") and (inspect.isclass(value) or typing.get_origin(value)):
                aliases[id(value)] = f"{name}.{key}"
    rows = {}
    trees = {}

    def add(path, value, kind, **extra):
        row = {"path": path, "kind": kind, **extra}
        if callable(value):
            row.update(inspect_callable(value, aliases))
        row["implementation"] = getattr(value, "__module__", type(value).__module__) + "." + getattr(
            value, "__qualname__", type(value).__qualname__)
        # Keep unreviewed destinations visible. An inventory is not a silently
        # approved migration map or proof of semantic documentation coverage.
        row["target_status"] = "needs_review"
        rows[path] = row

    def members(path, cls):
        if not inspect.isclass(cls) or not cls.__module__.startswith("zipzi"):
            return
        for name in sorted(instance_attributes(cls, trees)):
            rows[f"{path}.{name}"] = {"path": f"{path}.{name}", "kind": "instance_attribute",
                "annotation": "inspect constructor/assignment", "target_status": "needs_review"}
        for name, annotation in inspect.get_annotations(cls, eval_str=False).items():
            if not name.startswith("_"):
                rows[f"{path}.{name}"] = {"path": f"{path}.{name}", "kind": "field",
                    "annotation": annotation if isinstance(annotation, str) else aliases.get(
                        id(annotation), inspect.formatannotation(annotation)), "target_status": "needs_review"}
        if dataclasses.is_dataclass(cls):
            for field in dataclasses.fields(cls):
                if not field.name.startswith("_"):
                    rows[f"{path}.{field.name}"] = {"path": f"{path}.{field.name}", "kind": "field",
                        "annotation": field.type if isinstance(field.type, str) else aliases.get(
                            id(field.type), inspect.formatannotation(field.type)), "target_status": "needs_review"}
        if issubclass(cls, enum.Enum):
            for name, value in cls.__members__.items():
                rows[f"{path}.{name}"] = {"path": f"{path}.{name}", "kind": "enum_member",
                    "value": value.value, "target_status": "needs_review"}
        for name, value in inspect.getmembers_static(cls):
            if name.startswith("_") and name not in SPECIAL_METHODS:
                continue
            if isinstance(value, (classmethod, staticmethod)):
                value = getattr(cls, name)
            if isinstance(value, property):
                add(f"{path}.{name}", value.fget, "property")
            elif callable(value) and (getattr(value, "__module__", "") or "").startswith("zipzi"):
                add(f"{path}.{name}", value, "method")

    for name, module in modules.items():
        declared = getattr(module, "__all__", None)
        exports = set(declared or ()) | {
            key for key in vars(module) if not key.startswith("_")}
        for key in sorted(exports):
            path = f"{name}.{key}"
            if not hasattr(module, key):
                rows[path] = {"path": path, "kind": "missing_export", "target_status": "needs_review"}
                continue
            value = getattr(module, key)
            add(path, value, "export", explicit_all=declared is not None and key in declared)
            members(path, value)

    kernel = importlib.import_module("zipzi._kernel")
    facades = {"Runtime.sessions": kernel.Sessions}
    for path, cls in facades.items():
        add(f"zipzi.{path}", cls, "facade")
        members(f"zipzi.{path}", cls)
    return {"schema": 1, "status": "observed_not_accepted",
            "modules_without_all": sorted(name for name, module in modules.items() if not hasattr(module, "__all__")),
            "counts": dict(Counter(row["kind"] for row in rows.values())),
            "symbols": [rows[key] for key in sorted(rows)]}


class RecordingResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.records = []

    def addSuccess(self, test):
        super().addSuccess(test)
        self.records.append({"test": test.id(), "status": "passed"})

    def addFailure(self, test, error):
        super().addFailure(test, error)
        self.records.append({"test": test.id(), "status": "failed",
                             "detail": self._exc_info_to_string(error, test)})

    def addError(self, test, error):
        super().addError(test, error)
        self.records.append({"test": test.id(), "status": "error",
                             "detail": self._exc_info_to_string(error, test)})

    def addSubTest(self, test, subtest, error):
        super().addSubTest(test, subtest, error)
        self.records.append({"test": subtest.id(), "status": "passed" if error is None else "failed",
            **({} if error is None else {"detail": self._exc_info_to_string(error, test)})})

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.records.append({"test": test.id(), "status": "unverified", "reason": reason})

    def addExpectedFailure(self, test, error):
        super().addExpectedFailure(test, error)
        self.records.append({"test": test.id(), "status": "unverified",
            "reason": "Expected failures cannot satisfy a target contract",
            "detail": self._exc_info_to_string(error, test)})

    def addUnexpectedSuccess(self, test):
        super().addUnexpectedSuccess(test)
        self.records.append({"test": test.id(), "status": "failed",
            "reason": "Remove the expected-failure marker and verify normally"})


def baseline(output: Path, native: Path, host: Path, names: list[str]) -> bool:
    import os
    native_files = list((native / "zipzi_native").glob('_native' + sysconfig.get_config_var('EXT_SUFFIX')))
    if len(native_files) != 1 or not host.is_file():
        raise ValueError("Refresh fixtures with ./tools/test-sdk.sh python before running the baseline")
    os.environ.update(ZIPZI_KERNEL_NATIVE=str(native), ZIPZI_KERNEL_HOST=str(host),
                      ZIPZI_KERNEL_SOURCE_CLIENT=str(SOURCE))
    os.environ.pop("ZIPZI_DEV_SESSION_DIR", None)
    os.environ.pop("ZIPZI_KERNEL_EDITOR_HOST", None)
    sys.path.insert(0, str(PACKAGE / "tests"))
    module = importlib.import_module("test_kernel")
    loader = unittest.TestLoader()
    names = names or ["Values", "LocalLifecycle", "ConnectedLifecycle", "Recovery"]
    suite = loader.loadTestsFromNames(names, module)
    before = source_identity()
    runner_sha = sha256(Path(__file__))
    artifacts = {"native": artifact_identity(native_files[0]), "host": artifact_identity(host)}
    result = unittest.TextTestRunner(verbosity=2, resultclass=RecordingResult).run(suite)
    unchanged = before == source_identity() and runner_sha == sha256(Path(__file__)) and artifacts == {
        "native": artifact_identity(native_files[0]), "host": artifact_identity(host)}
    passed = accepted(result, unchanged)
    write_json(output, {"schema": 1, "status": "passed" if passed else "failed",
        "scope": "source_target_regressions_only", "started_source": before,
        "runner_sha256": runner_sha, "selected_tests": names,
        "inputs_unchanged": unchanged, "artifacts": artifacts,
        "tests_run": result.testsRun, "results": result.records})
    return passed


def accepted(result: unittest.TestResult, unchanged: bool) -> bool:
    return (result.wasSuccessful() and result.testsRun > 0 and not result.skipped
            and not result.expectedFailures and unchanged)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("inventory", "freeze", "baseline"):
        command = commands.add_parser(name)
        command.add_argument("--output", required=True, type=Path, help="New JSON evidence file; never overwritten")
        if name in {"freeze", "baseline"}:
            command.add_argument("--native", type=Path, default=ROOT / "target/python/native")
            command.add_argument("--host", type=Path, default=ROOT / 'target/python' / ('local_host_fixture.exe' if sys.platform == 'win32' else 'local_host_fixture'))
        if name == "freeze":
            command.add_argument("--editor", type=Path, default=ROOT / "target/debug/zipzi-editor")
            command.add_argument("--wheel", type=Path, action="append", default=[], help="Exact installed/build wheel to hash; repeatable")
        if name == "baseline":
            command.add_argument("tests", nargs="*", help="Optional v4 test selectors such as Recovery")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists; select a new evidence path")
    if args.command == "inventory":
        write_json(args.output, inventory())
    elif args.command == "freeze":
        native_files = sorted((args.native / "zipzi_native").glob('_native' + sysconfig.get_config_var('EXT_SUFFIX')))
        write_json(args.output, {"schema": 1, "status": "frozen_not_accepted",
            "created_at_utc": datetime.now(timezone.utc).isoformat(),
            "python": sys.version, "platform": platform.platform(), "source": source_identity(),
            "artifacts": {"native": [artifact_identity(path) for path in native_files],
                "host": artifact_identity(args.host), "editor": artifact_identity(args.editor),
                "wheels": [artifact_identity(path) for path in args.wheel]},
            "capability_evidence": {"path": "crates/zipzi-python/CAPABILITIES.md",
                "sha256": sha256(PACKAGE / "CAPABILITIES.md"),
                "meaning": "current documented capabilities"},
            "inventory": inventory()})
    else:
        if not baseline(args.output, args.native.resolve(), args.host.resolve(), args.tests):
            raise SystemExit(1)
    print(args.output)


if __name__ == "__main__":
    main()
