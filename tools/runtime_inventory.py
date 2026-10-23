#!/usr/bin/env python3
"""Reproducible R0 source inventory; classification is not connection coverage.

Scan every split SDK implementation, re-exported animation implementation,
public Python facade method/value, and native stub type. Never import _native.
The checked-in catalog is a review artifact. --check detects source drift.
"""
from __future__ import annotations

import argparse
import ast
from collections import Counter
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/runtime/api-inventory.json"
RAW_STRING = re.compile(r'(?:br|r)(#+)?"')
CHAR_LITERAL = re.compile(r"'(?:\\.|[^'\\\n])'")

# Domain contracts specify source, history, ownership, bulk work and acceptance.
DOMAINS = {
    "session": ("session-history", "R3", ["A02", "A03", "A04", "A05", "A10"]),
    "project_io": ("resources-project", "R4", ["A02", "A08", "A14", "A17"]),
    "assets": ("resources-project", "R4", ["A14", "A17"]),
    "resource_authoring": ("resources-project", "R4", ["A14", "A17"]),
    "document_authoring": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "authoring_collections": ("identity-collections", "R4", ["A04", "A05", "A15"]),
    "geometry": ("geometry-binding", "R4", ["A04", "A05", "A15"]),
    "alpha_mesh": ("geometry-binding", "R4", ["A04", "A15", "A17"]),
    "binding_authoring": ("geometry-binding", "R4", ["A04", "A05", "A15", "A17"]),
    "scene_authoring": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "scene_content": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "organization_authoring": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "reference_authoring": ("identity-collections", "R4", ["A04", "A05", "A15"]),
    "diagnostics": ("cpu-evaluation", "R4", ["A14", "A15"]),
    "spatial": ("observe-preview", "R5", ["A11", "A14", "A15"]),
    "expression_edit": ("animation", "R5", ["A04", "A05", "A11"]),
    "motion_edit": ("animation", "R5", ["A04", "A05", "A11"]),
    "motion_authoring": ("animation", "R5", ["A04", "A05", "A11"]),
    "physics_authoring": ("animation", "R5", ["A04", "A05", "A11"]),
    "pose_authoring": ("animation", "R5", ["A04", "A05", "A11"]),
    "changes": ("events", "R3", ["A03", "A10"]),
    "types": ("values-identity", "R3", ["A05", "A12"]),
    "edit": ("scene-document", "R4", ["A03", "A04", "A05", "A15"]),
}


def mask_rust(source: str) -> str:
    """Mask comments/literals without moving source offsets, including raw strings.

    This is a source indexer, not a Rust compiler. Lifetimes remain intact.
    Unexpected impl syntax fails rather than silently losing methods.
    """
    result = list(source)
    i = 0
    while i < len(source):
        start = i
        if source.startswith("//", i):
            i = source.find("\n", i)
            if i < 0:
                i = len(source)
        elif source.startswith("/*", i):
            depth, i = 1, i + 2
            while depth and i < len(source):
                if source.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif source.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            if depth:
                raise ValueError("unterminated block comment")
        elif source[i] in "br" and (match := RAW_STRING.match(source, i)):
            ending = '"' + (match[1] or "")
            end = source.find(ending, i + len(match[0]))
            if end < 0:
                raise ValueError("unterminated raw string")
            i = end + len(ending)
        elif source[i] == '"' or (source[i] == "'" and CHAR_LITERAL.match(source, i)):
            quote, i = source[i], i + 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == quote:
                    i += 1
                    break
                else:
                    i += 1
        else:
            i += 1
            continue
        for j in range(start, i):
            if source[j] != "\n":
                result[j] = " "
    return "".join(result)


def closing(mask: str, start: int) -> int:
    depth = 0
    for i in range(start, len(mask)):
        depth += (mask[i] == "{") - (mask[i] == "}")
        if not depth:
            return i
    raise ValueError("unbalanced Rust braces")


def rust_methods(source: str) -> list[dict]:
    masked = mask_rust(source)
    impls = []
    for match in re.finditer(r"(?m)^\s*impl\b([^{}]+)\{", masked):
        header = match[1].strip()
        owner = re.search(r"(?:^|\s)(?:crate::)?(\w+)(?:<[^>]*>)?\s*$", header)
        if owner is None:
            raise ValueError(f"unrecognized impl: {header}")
        impls.append((match.end(), closing(masked, match.end() - 1), owner[1]))
    result = []
    for match in re.finditer(r"\bpub\s+(?:async\s+|const\s+)?fn\s+(\w+)", masked):
        start = masked.find("{", match.end())
        if start < 0:
            raise ValueError("public function without body")
        owner = next((owner for a, b, owner in reversed(impls) if a <= match.start() < b), "free")
        signature = " ".join(source[match.start():start].split())
        result.append({"owner": owner, "name": match[1], "signature": signature,
                       "line": source.count("\n", 0, match.start()) + 1})
    return result


def rust_contract(row: dict, path: Path) -> dict:
    owner, name = row["owner"], row["name"]
    module = path.relative_to(ROOT / "modules/kasane-sdk/src").parts[0].removesuffix(".rs")
    if module not in DOMAINS:
        raise ValueError(f"classify new SDK module: {module}")
    domain, stage, acceptance = DOMAINS[module]
    if module == "edit" and path.parent.name == "edit":
        if path.stem in {"motion", "expression", "physics", "pose"}:
            domain, stage, acceptance = DOMAINS["motion_authoring"]
        elif path.stem == "model3":
            domain, stage, acceptance = DOMAINS["project_io"]
    classification = "connected"
    effect = "read-only"
    source = "session"
    lifecycle = "versioned value; handle incarnation/address epoch checked by SDK"
    if owner == "EditSession":
        source, effect = "edit + candidate sequence", "candidate only; ordinary error poisons edit"
        lifecycle = "client/session owned Edit; commit consumes; cancel drops; no-op preserves redo"
        if name in {"base_version", "candidate_document"}:
            effect = "read-only candidate"
        elif name in {"commit", "commit_to"}:
            effect, stage = "one history step iff changed; SDK validates complete Version", "R3"
        elif name == "preview_property":
            effect = "recoverable candidate preview; rejected update retains legal candidate"
    elif owner == "AuthoringSnapshot":
        source, lifecycle = "snapshot (+ candidate sequence for derived source)", "immutable captured Version; explicit release/budget"
    elif owner in {"MotionClipCandidate", "ExpressionEntryCandidate"}:
        source, effect = "edit + candidate sequence", "candidate only; dedicated recoverable update contract"
    elif owner == "AuthoringSession":
        if name in {"new", "with_history_limits", "with_filesystem"}:
            source, effect = "none", "new session; no history"
            if name != "new":
                classification = "host-only"
                lifecycle = "Rust host configuration; not a callable client filesystem or closure"
        elif name in {"begin_edit", "begin_owned_edit", "edit", "begin_expression_entry_edit", "begin_image_content_edit"}:
            effect = "begin candidate; edit convenience commits once iff changed"
            stage = "R3"
        elif name == "drain_events":
            classification, effect = "host-only", "consumes SDK events exactly once in Runtime; clients subscribe instead"
        elif name in {"undo", "redo"} or name.startswith("execute_") or "EditReceipt" in row["signature"]:
            effect = "formal publication; SDK owns history; no-op/error preserves redo"
        elif name.startswith("import_") or name in {"open_project", "new_project"}:
            effect = "session replacement; generation advances; history reset per SDK"
        elif name == "save_project":
            effect = "exclusive IO; retain history; relocate resources; update saved baseline"
        elif name.startswith("set_preview") or name == "reset_preview_values":
            source, effect = "client default Preview", "preview state only; never formal history"
        elif "preview" in name:
            source, lifecycle = "client default Preview or frozen Preview source", "independent client Preview in R5"
            stage = "R5"
        elif name == "prepare_rgba_asset":
            effect = "retains resource payload; no formal revision/history; R4 accounting/reclamation required"
        elif name in {"image_load_token", "attachment_load_token"}:
            effect = "registers cancellable resource operation; replacing token can cancel earlier load; no formal history"
        elif name.startswith("set_"):
            effect = "host configuration; no formal history"
    elif owner in {"ImageLoadToken", "AttachmentLoadToken"}:
        effect = "cooperative cancellation; reservation retained until confirmed terminal completion"
        lifecycle = "owned operation token; cancellation routes to host"
    elif owner != "free":
        classification, source = "local-value", "returned value/token metadata"
        lifecycle = "copy metadata locally; remote Plan/Address still checked on execution/release"
    if owner == "free":
        source = "explicit arguments"
        classification = "connected" if name.startswith("prepare_") or module == "spatial" else "local-helper"
        lifecycle = "value result; host resource IO uses blob/path policy; no native pointer on wire"
    if name.startswith("plan_") or owner.endswith("Plan"):
        lifecycle = "Plan owned by client/session/source; base Version checked at execute; explicit release"
    expensive = (name.startswith(("import_", "export_", "plan_", "execute_", "prepare_"))
                 or name in {"save_project", "open_project", "commit", "commit_to", "read_snapshot", "remesh_rectangle_grid"})
    return dict(domain=domain, classification=classification, stage=stage,
                source_ref=source, effect=effect, lifecycle=lifecycle,
                bulk="preserve SDK collection/array arguments; add explicit batch queries in R4",
                scheduling="measure/worker ownership or captured source" if expensive else "short typed call; measure before UI migration",
                acceptance=acceptance, implementation="not-connected")


def collect(root: Path = ROOT) -> dict:
    # root is injected in tests only for the independent scanner functions.
    if root != ROOT:
        raise ValueError("inventory must describe this paired checkout")
    rows = []
    legacy = json.loads((ROOT / "modules/kasane-python/python-coverage.json").read_text())
    sdk_paths = sorted((ROOT / "modules/kasane-sdk/src").rglob("*.rs"))
    public_owners = {name for path in sdk_paths for name in re.findall(
        r"\bpub\s+(?:struct|enum)\s+(\w+)", mask_rust(path.read_text()))}
    for path in sdk_paths:
        for method in rust_methods(path.read_text()):
            owner, name = method["owner"], method["name"]
            if owner != "free" and owner not in public_owners:
                continue
            entry = dict(id=f"sdk::{owner}.{name}", location=str(path.relative_to(ROOT)), **method,
                         **rust_contract(method, path))
            legacy_owner = {"AuthoringSession": "Session", "EditSession": "Edit"}.get(owner, owner)
            previous = legacy.get(f"{legacy_owner}.{name}")
            if previous:
                entry["legacy_binding_evidence"] = previous
            rows.append(entry)

    # Re-exports are part of the public SDK, although their impl lives elsewhere.
    sdk_lib = (ROOT / "modules/kasane-sdk/src/lib.rs").read_text()
    animation_exports = set()
    for names in re.findall(r"pub use kasane_animation::\{([^}]+)\};", sdk_lib, re.S):
        animation_exports.update(re.findall(r"\b\w+\b", names))
    animation_exports.update(re.findall(r"pub use kasane_animation::(\w+);", sdk_lib))
    for path in sorted((ROOT / "modules/kasane-animation/src").rglob("*.rs")):
        for method in rust_methods(path.read_text()):
            if method["owner"] not in animation_exports and method["name"] not in animation_exports:
                continue
            rows.append(dict(id=f"sdk::{method['owner']}.{method['name']}",
                location=str(path.relative_to(ROOT)), **method, domain="observe-preview",
                classification="local-helper" if method["owner"] in {"free", "CompiledCurve", "CompiledSegment"} else "connected",
                stage="R5", source_ref="explicit curve arguments" if method["owner"] in {"free", "CompiledCurve", "CompiledSegment"} else "independent frozen Preview",
                effect="no formal history", lifecycle="client-owned Preview or local curve value; explicit release",
                bulk="ordered schedule and sample arrays", scheduling="captured source; bounded replay worker",
                acceptance=["A11", "A14", "A17"], implementation="not-connected"))

    package = ROOT / "modules/kasane-python/python/kasane"
    native_imports = []
    for path in sorted(package.glob("*.py")):
        tree = ast.parse(path.read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and node.module and "_native" in node.module:
                native_imports.append(dict(location=str(path.relative_to(ROOT)), line=node.lineno,
                    names=[n.name for n in node.names]))
        for node in tree.body:
            if isinstance(node, ast.ClassDef):
                if node.name.startswith("_") or node.name == "SpatialSource":
                    continue
                members = [member for member in node.body if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))
                           and (not member.name.startswith("_") or member.name in {"__init__", "__enter__", "__exit__"})]
                fields = [ast.unparse(member) for member in node.body if isinstance(member, ast.AnnAssign)]
                if fields:
                    rows.append(dict(id=f"python::{node.name}", location=str(path.relative_to(ROOT)), line=node.lineno,
                        signature="; ".join(fields), domain="values-identity", classification="local-value",
                        stage="R3", source_ref="returned value", effect="local record", lifecycle="Python value",
                        bulk="record/array", scheduling="local", acceptance=["A12"], implementation="not-connected"))
                for member in members:
                    rows.append(python_row(path, member, node.name))
            elif isinstance(node, ast.FunctionDef) and not node.name.startswith("_"):
                rows.append(python_row(path, node, "free"))
    # Include the whole native type boundary, not only names imported by __init__.
    stub = ast.parse((package / "_native.pyi").read_text())
    native_types = [{"name": node.name, "line": node.lineno,
        "members": [m.name for m in node.body if isinstance(m, ast.FunctionDef)]}
        for node in stub.body if isinstance(node, ast.ClassDef)]
    init = ast.parse((package / "__init__.py").read_text())
    public_exports = next(ast.literal_eval(node.value) for node in init.body if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets))
    for node in stub.body:
        if isinstance(node, ast.FunctionDef) and node.name in public_exports:
            rows.append(python_row(package / "_native.pyi", node, "free"))
        elif isinstance(node, ast.ClassDef) and node.name in {"ObjectHandle", "SdkFailure", "ObservationFailure"}:
            fields = [ast.unparse(member) for member in node.body if isinstance(member, ast.AnnAssign)]
            if fields:
                rows.append(dict(id=f"python::{node.name}", location=str((package / "_native.pyi").relative_to(ROOT)),
                    line=node.lineno, signature="; ".join(fields), domain="values-identity", classification="local-value",
                    stage="R3", source_ref="structured error value", effect="no history", lifecycle="Python value",
                    bulk="structured diagnostic", scheduling="local", acceptance=["A12"], implementation="not-connected"))
            for member in node.body:
                if isinstance(member, ast.FunctionDef) and not member.name.startswith("_"):
                    rows.append(python_row(package / "_native.pyi", member, node.name))
    facade = [row for row in rows if row["id"].startswith("python::")]
    for row in rows:
        if row["id"].startswith("sdk::"):
            # Same-name matches are leads for implementation review, not proof of
            # an equivalent public binding. Preserve the old evidence separately.
            row["facade_route_candidates"] = [item["id"] for item in facade
                if row["name"] in item.get("native_calls", [])
                or row["name"] == item.get("name")]
            row["facade_route_status"] = "review candidate routes" if row["facade_route_candidates"] else "new route or explicit adapter required"
    identities = [row["id"] for row in rows]
    duplicates = [key for key, count in Counter(identities).items() if count > 1]
    if duplicates:
        raise ValueError(f"duplicate API identities: {duplicates}")
    return dict(schema=1, note="R0 classification and target contracts, NOT implemented connection coverage",
                apis=sorted(rows, key=lambda row: row["id"]), native_imports=native_imports,
                native_types=native_types, python_public_exports=public_exports,
                domain_intents=domain_intents(), value_types=value_types())


def value_types() -> list[dict]:
    """Index public input/result fields; opaque objects are references, not DTOs."""
    result = []
    sdk_lib = (ROOT / "modules/kasane-sdk/src/lib.rs").read_text()
    for crate in ["kasane-sdk", "kasane-animation", "kasane-project"]:
        exported = set()
        if crate != "kasane-sdk":
            rust_name = crate.replace("-", "_")
            for names in re.findall(r"pub use " + rust_name + r"::\{([^}]+)\};", sdk_lib, re.S):
                exported.update(re.findall(r"\b\w+\b", names))
            exported.update(re.findall(r"pub use " + rust_name + r"::(\w+);", sdk_lib))
        for path in sorted((ROOT / "modules" / crate / "src").rglob("*.rs")):
            source = path.read_text()
            masked = mask_rust(source)
            for match in re.finditer(r"\bpub\s+(enum|struct)\s+(\w+)\b[^{};]*\{", masked):
                name = match[2]
                if (crate != "kasane-sdk" and name not in exported) or name.endswith(("Edit", "Operation")):
                    continue
                end = closing(masked, match.end() - 1)
                reference = name.endswith(("Plan", "Address", "Token")) or name in {
                    "AuthoringSession", "AuthoringSnapshot", "EditSession", "MotionClipCandidate",
                    "ExpressionEntryCandidate", "ExpressionPreview", "MotionPreview", "PhysicsPreview", "MotionEditingPreview", "ObjectHandle"}
                row = dict(name=name, kind=match[1], location=str(path.relative_to(ROOT)),
                    line=source.count("\n", 0, match.start()) + 1,
                    classification="backend-reference" if reference else "local-value",
                    contract="route opaque lifetime/version state; do not serialize Rust/private fields" if reference else "explicit public value DTO; not native memory layout")
                if not reference:
                    row["definition"] = source[match.start():end + 1]
                result.append(row)
    return sorted(result, key=lambda row: (row["name"], row["location"]))


def domain_intents() -> list[dict]:
    """Include the actual shared SDK/core intent variants, not private PyO3 Command."""
    result = []
    for directory in [ROOT / "modules/kasane-sdk/src", ROOT / "modules/kasane-core/src/document"]:
        for path in sorted(directory.rglob("*.rs")):
            source = path.read_text()
            masked = mask_rust(source)
            for match in re.finditer(r"\bpub\s+(enum|struct)\s+(\w+(?:Edit|Operation))\b[^{};]*\{", masked):
                end = closing(masked, match.end() - 1)
                result.append(dict(name=match[2], kind=match[1], location=str(path.relative_to(ROOT)),
                    line=source.count("\n", 0, match.start()) + 1,
                    definition=source[match.start():end + 1], classification="connected",
                    contract="same SDK intent, atomic history on commit; Plan/source/version checks; no private Command protocol",
                    implementation="not-connected"))
    return sorted(result, key=lambda row: row["name"])


def python_row(path: Path, node: ast.FunctionDef, owner: str) -> dict:
    name = node.name
    native_calls = sorted({call.func.attr for call in ast.walk(node) if isinstance(call, ast.Call)
                          and isinstance(call.func, ast.Attribute)
                          and "_native" in ast.unparse(call.func.value)})
    domain, stage, acceptance = DOMAINS["session"]
    classification, effect, source_ref = "connected", "read-only or explicit backend operation", "session"
    if owner == "Edit":
        source_ref, effect = "edit + candidate sequence", "candidate; commit once iff changed"
        stage = "R4"
    elif owner in {"MotionPreview", "PhysicsPreview", "ExpressionPreview", "Observer", "CapturedScene", "RenderedSceneView", "FocusedSceneView"}:
        domain, stage, acceptance = DOMAINS["spatial"]
        source_ref, effect = "independent Preview / captured source", "no formal history"
    elif name == "capabilities":
        source_ref, acceptance = "host/backend metadata (no Session required)", ["A06", "A12"]
        effect = "query attached host/backend capabilities; native build flags are not connected host flags"
    elif path.stem == "_mesh_edit" or name == "open_project":
        domain, stage, acceptance = DOMAINS["geometry"] if path.stem == "_mesh_edit" else DOMAINS["session"]
        effect = "formal publication" if path.stem == "_mesh_edit" else "open/bind session through registry"
    elif owner == "ObjectHandle":
        classification, effect, source_ref = "local-value", "read token metadata", "returned handle"
    elif owner != "Session" or path.stem == "geometry":
        classification, effect, source_ref = "local-helper", "local value/file operation", "captured value / explicit arguments"
        stage = "R7"
    if owner == "Session" and any(part in name for part in ("motion", "expression", "physics", "pose")):
        domain, stage, acceptance = DOMAINS["motion_authoring"]
    elif owner == "Session" and any(part in name for part in ("binding", "mesh", "blend", "geometry")):
        domain, stage, acceptance = DOMAINS["geometry"]
    elif owner == "Session" and any(part in name for part in ("asset", "resource", "save", "export", "import", "attachment")):
        domain, stage, acceptance = DOMAINS["project_io"]
    elif owner == "Session" and name not in {"__init__", "edit", "version", "history_state", "undo", "redo", "modified", "drain_events"}:
        stage = "R4"
    return dict(id=f"python::{owner}.{name}" if owner != "free" else f"python::{path.stem}.{name}",
        location=str(path.relative_to(ROOT)), line=node.lineno, owner=owner, name=name,
        signature=f"def {name}({ast.unparse(node.args)})" + (f" -> {ast.unparse(node.returns)}" if node.returns else ""),
        description=(ast.get_docstring(node) or "").split("\n")[0],
        native_calls=native_calls, domain=domain, classification=classification, stage=stage,
        source_ref=source_ref, effect=effect,
        lifecycle="backend refs become client/session-owned tokens; records stay values",
        bulk="preserve Sequence/Mapping input and collection results",
        scheduling="backend decides from SDK source/operation; Python conversion stays local",
        acceptance=acceptance, implementation="not-connected")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    catalog = collect()
    # One row per API keeps source reviews manageable while retaining valid JSON.
    encoded = "{\n" + ",\n".join(
        f"  {json.dumps(key)}: [\n" + ",\n".join("    " + json.dumps(row, ensure_ascii=False) for row in value) + "\n  ]"
        if isinstance(value, list) else f"  {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}"
        for key, value in catalog.items()) + "\n}\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != encoded:
            print("Runtime source inventory changed; review and regenerate tools/runtime_inventory.py")
            return 1
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(encoded)
    print(json.dumps(dict(apis=len(catalog["apis"]), classifications=Counter(row["classification"] for row in catalog["apis"]),
                          native_types=len(catalog["native_types"])), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
