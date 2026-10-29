#!/usr/bin/env python3
"""Reproducible R0 source inventory; classification is not connection coverage.

Scan every split SDK implementation, re-exported animation implementation,
historical Python facade rows retained in the checked-in catalog.
The catalog is a review artifact. --check detects shared source drift.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "docs/archive/plans/runtime/api-inventory.json"
RAW_STRING = re.compile(r'(?:br|r)(#+)?"')
CHAR_LITERAL = re.compile(r"'(?:\\.|[^'\\\n])'")

# Domain contracts specify source, history, ownership, bulk work and acceptance.
DOMAINS = {
    "modeling": ("modeling-batch", "R4", ["A04", "A05", "A14", "A15", "A17"]),
    "session": ("session-history", "R3", ["A02", "A03", "A04", "A05", "A10"]),
    "project_io": ("resources-project", "R4", ["A02", "A08", "A14", "A17"]),
    "assets": ("resources-project", "R4", ["A14", "A17"]),
    "resource_authoring": ("resources-project", "R4", ["A14", "A17"]),
    "image_edit": ("resources-project", "R4", ["A04", "A14", "A17"]),
    "mesh_edit": ("geometry-binding", "R4", ["A04", "A05", "A15"]),
    "mesh_binding_edit": ("geometry-binding", "R4", ["A04", "A05", "A15"]),
    "parameter_edit": ("scene-document", "R3", ["A03", "A04", "A05"]),
    "document_authoring": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "authoring_collections": ("identity-collections", "R4", ["A04", "A05", "A15"]),
    "geometry": ("geometry-binding", "R4", ["A04", "A05", "A15"]),
    "manual_mesh": ("geometry-binding", "R4", ["A04", "A05", "A15"]),
    "alpha_mesh": ("geometry-binding", "R4", ["A04", "A15", "A17"]),
    "binding_authoring": ("geometry-binding", "R4", ["A04", "A05", "A15", "A17"]),
    "scene_authoring": ("scene-document", "R4", ["A04", "A05", "A15"]),
    "scene_edit": ("scene-document", "R4", ["A04", "A05", "A15"]),
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
    module = path.relative_to(ROOT / "crates/zipzi-sdk/src").parts[0].removesuffix(".rs")
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
        if name in {"base_version", "candidate_document", "is_aborted", "estimated_bytes", "memory_image"}:
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
        elif name in {"begin_edit", "begin_owned_edit", "edit", "begin_expression_entry_edit", "begin_image_content_edit",
                      "begin_import_prepared_image", "begin_prepared_image_content_edit",
                      "begin_attachment_content_edit", "begin_remesh_rectangle_grid"}:
            effect = "begin candidate; edit convenience commits once iff changed"
            stage = "R3"
        elif name == "candidate_snapshot":
            source, effect = "edit + candidate sequence", "immutable candidate capture; never formal publication"
            lifecycle = "same SDK session/generation; retains memory images; disk bytes not pinned until R4"
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
        elif name == "prepare_rgba_image":
            effect = "returns an owned image lease; no registry entry or formal revision/history"
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
    if module == "modeling" and (name in {"apply", "modeling_records"}):
        classification, source = "connected", "explicit formal/candidate source"
        effect = "read-only records" if name == "modeling_records" else "SDK candidate operation; no independent publication"
    if owner in {"MeshOperation", "MeshBindingOperation", "ParameterOperation", "SceneOperation"} and name == "apply":
        classification, source = "connected", "edit + candidate sequence"
        effect = "candidate only; ordinary error poisons edit"
        lifecycle = "SDK intent applied through the owning Edit; no independent publication"
    if owner == "SceneSummary" or (owner == "SceneOperation" and name == "into_intent"):
        classification = "host-only"
        source = "borrowed formal/candidate document or typed scene input"
        effect = "metadata projection/admission or SDK intent construction; no publication"
        lifecycle = "Rust adapter helper; public Python uses typed values and Runtime handlers"
    if owner in {"ModelingSampler", "SpatialSampler"}:
        classification, source = "local-helper", "immutable borrowed/owned authoring snapshot"
        effect = "SDK resumable sampling; no formal publication"
        lifecycle = "worker-owned continuation; discard after result/error; not a client token"
    if name.startswith("plan_") or owner.endswith("Plan"):
        lifecycle = "Plan owned by client/session/source; base Version checked at execute; explicit release"
    expensive = (name.startswith(("import_", "export_", "plan_", "execute_", "prepare_"))
                 or name in {"save_project", "open_project", "commit", "commit_to", "read_snapshot", "candidate_snapshot", "remesh_rectangle_grid"}
                 or owner in {"ModelingSampler", "SpatialSampler"})
    if owner == "AuthoringSession" and name == "check_version":
        classification = "host-only"
        effect = "version admission check; no mutation"
    return dict(domain=domain, classification=classification, stage=stage,
                source_ref=source, effect=effect, lifecycle=lifecycle,
                bulk="preserve SDK collection/array arguments; add explicit batch queries in R4",
                scheduling="measure/worker ownership or captured source" if expensive else "short typed call; measure before UI migration",
                acceptance=acceptance, implementation="not-connected")


def collect(root: Path = ROOT, *, shared_only: bool = False) -> dict:
    # root is injected in tests only for the independent scanner functions.
    if root != ROOT:
        raise ValueError("inventory must describe this paired checkout")
    rows = []
    sdk_paths = sorted((ROOT / "crates/zipzi-sdk/src").rglob("*.rs"))
    public_owners = {name for path in sdk_paths for name in re.findall(
        r"\bpub\s+(?:struct|enum)\s+(\w+)", mask_rust(path.read_text()))}
    for path in sdk_paths:
        for method in rust_methods(path.read_text()):
            owner, name = method["owner"], method["name"]
            if owner != "free" and owner not in public_owners:
                continue
            entry = dict(id=f"sdk::{owner}.{name}", location=str(path.relative_to(ROOT)), **method,
                         **rust_contract(method, path))
            rows.append(entry)

    # Re-exports are part of the public SDK, although their impl lives elsewhere.
    sdk_lib = (ROOT / "crates/zipzi-sdk/src/lib.rs").read_text()
    animation_exports = set()
    for names in re.findall(r"pub use zipzi_animation::\{([^}]+)\};", sdk_lib, re.S):
        animation_exports.update(re.findall(r"\b\w+\b", names))
    animation_exports.update(re.findall(r"pub use zipzi_animation::(\w+);", sdk_lib))
    for path in sorted((ROOT / "crates/zipzi-animation/src").rglob("*.rs")):
        for method in rust_methods(path.read_text()):
            if method["owner"] not in animation_exports and method["name"] not in animation_exports:
                continue
            rows.append(dict(id=f"sdk::{method['owner']}.{method['name']}",
                location=str(path.relative_to(ROOT)), **method, domain="observe-preview",
                # Runtime consumes this estimate for admission; it is not a
                # separate client operation or an omitted Preview capability.
                classification="host-only" if method["owner"] == "MotionPreview" and method["name"] == "canonical_motion_output_bytes" else
                    "local-helper" if method["owner"] in {"free", "CompiledCurve", "CompiledSegment"} or method["name"] == "matches_document" else "connected",
                stage="R5", source_ref="explicit curve arguments" if method["owner"] in {"free", "CompiledCurve", "CompiledSegment"} else "independent frozen Preview",
                effect="no formal history", lifecycle="client-owned Preview or local curve value; explicit release",
                bulk="ordered schedule and sample arrays", scheduling="captured source; bounded replay worker",
                acceptance=["A11", "A14", "A17"], implementation="not-connected"))

    # Refresh shared Rust sources without inspecting the retired client.
    # Historical Python rows remain historical evidence, not a new API spec.
    previous = json.loads(OUTPUT.read_text())
    facade = [row for row in previous["apis"] if row["id"].startswith("python::")]
    old_rows = {row["id"]: row for row in previous["apis"]}
    for row in rows:
        old = old_rows.get(row["id"], {})
        if "legacy_binding_evidence" in old:
            row["legacy_binding_evidence"] = old["legacy_binding_evidence"]
        row["facade_route_candidates"] = [item["id"] for item in facade
            if row["name"] in item.get("native_calls", []) or row["name"] == item.get("name")]
        row["facade_route_status"] = "review candidate routes" if row["facade_route_candidates"] else "new route or explicit adapter required"
    return dict(previous, apis=sorted(rows + facade, key=lambda row: row["id"]),
                domain_intents=domain_intents(), value_types=value_types())



def value_types() -> list[dict]:
    """Index public input/result fields; opaque objects are references, not DTOs."""
    result = []
    sdk_lib = (ROOT / "crates/zipzi-sdk/src/lib.rs").read_text()
    for crate in ["zipzi-sdk", "zipzi-animation", "zipzi-project"]:
        exported = set()
        if crate != "zipzi-sdk":
            rust_name = crate.replace("-", "_")
            for names in re.findall(r"pub use " + rust_name + r"::\{([^}]+)\};", sdk_lib, re.S):
                exported.update(re.findall(r"\b\w+\b", names))
            exported.update(re.findall(r"pub use " + rust_name + r"::(\w+);", sdk_lib))
        for path in sorted((ROOT / "crates" / crate / "src").rglob("*.rs")):
            source = path.read_text()
            masked = mask_rust(source)
            for match in re.finditer(r"\bpub\s+(enum|struct)\s+(\w+)\b[^{};]*\{", masked):
                name = match[2]
                if (crate != "zipzi-sdk" and name not in exported) or name.endswith(("Edit", "Operation")):
                    continue
                end = closing(masked, match.end() - 1)
                reference = name.endswith(("Plan", "Address", "Token")) or name in {
                    "AuthoringSession", "AuthoringSnapshot", "EditSession", "MotionClipCandidate",
                    "ExpressionEntryCandidate", "ExpressionPreview", "MotionPreview", "PhysicsPreview", "MotionEditingPreview", "ObjectHandle"}
                row = dict(name=name, kind=match[1], location=str(path.relative_to(ROOT)),
                    line=source.count("\n", 0, match.start()) + 1,
                    classification="backend-reference" if reference else "local-value",
                    contract="route opaque lifetime/version state; do not serialize Rust/private fields" if reference else "explicit public value DTO; not native memory layout")
                if name in {"ModelingSampler", "SpatialSampler"}:
                    row.update(classification="local-helper", contract="SDK worker-owned sampling state; not a serializable DTO or client token")
                elif not reference:
                    row["definition"] = source[match.start():end + 1]
                result.append(row)
    return sorted(result, key=lambda row: (row["name"], row["location"]))


def domain_intents() -> list[dict]:
    """Include the actual shared SDK/core intent variants, not private PyO3 Command."""
    result = []
    for directory in [ROOT / "crates/zipzi-sdk/src", ROOT / "crates/zipzi-core/src/document"]:
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--shared-only", action="store_true", help="Refresh/check shared Rust sources, retaining historical Python catalog rows without reading the retired package")
    args = parser.parse_args()
    catalog = collect(shared_only=args.shared_only)
    # One row per API keeps source reviews manageable while retaining valid JSON.
    encoded = "{\n" + ",\n".join(
        f"  {json.dumps(key)}: [\n" + ",\n".join("    " + json.dumps(row, ensure_ascii=False) for row in value) + "\n  ]"
        if isinstance(value, list) else f"  {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}"
        for key, value in catalog.items()) + "\n}\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != encoded:
            print("Runtime source inventory changed; review and regenerate tools/runtime/runtime_inventory.py")
            return 1
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(encoded)
    print(json.dumps(dict(apis=len(catalog["apis"]), classifications=Counter(row["classification"] for row in catalog["apis"]),
                          native_types=len(catalog["native_types"])), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
