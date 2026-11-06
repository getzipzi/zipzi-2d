#!/usr/bin/env python3
"""Inventory Rust SDK declarations without inferring client route coverage."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
RAW_STRING = re.compile(r'(?:br|r)(#+)?"')
CHAR_LITERAL = re.compile(r"'(?:\\.|[^'\\\n])'")


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
        trait_match = re.match(r"(?:crate::)?(\w+)\s+for\s+", header)
        trait_name = trait_match[1] if trait_match else None
        owner = re.search(r"(?:^|\s)(?:crate::)?(\w+)(?:<[^>]*>)?\s*$", header)
        if owner is None:
            raise ValueError(f"unrecognized impl: {header}")
        impls.append((match.end(), closing(masked, match.end() - 1), owner[1], trait_name))
    result = []
    for match in re.finditer(r"\b(?:pub\s+)?(?:async\s+|const\s+)?fn\s+(\w+)", masked):
        start = masked.find("{", match.end())
        if start < 0:
            raise ValueError("public function without body")
        matching_impl = next(((owner, trait) for a, b, owner, trait in reversed(impls) if a <= match.start() < b), None)
        owner = matching_impl[0] if matching_impl else "free"
        trait = matching_impl[1] if matching_impl else None
        is_pub = match.group(0).startswith("pub")
        if not is_pub and (trait is None or not (trait.endswith(("Apply", "Read")) or "Validation" in trait)):
            continue
        signature = " ".join(source[match.start():start].split())
        result.append({"owner": owner, "name": match[1], "signature": signature,
                       "line": source.count("\n", 0, match.start()) + 1})
    return result


def collect() -> dict:
    sdk = ROOT / "crates/zipzi-sdk/src"
    sources = {path: path.read_text(encoding="utf-8") for path in sorted(sdk.rglob("*.rs"))}
    public_owners = {name for source in sources.values() for name in re.findall(
        r"\bpub\s+(?:struct|enum)\s+(\w+)", mask_rust(source))}
    apis = [
        dict(id=f"sdk::{row['owner']}.{row['name']}", location=str(path.relative_to(ROOT)), **row)
        for path, source in sources.items()
        for row in rust_methods(source)
        if row['owner'] == 'free' or row['owner'] in public_owners
    ]
    return {"scope": "sdk_source_declarations", "apis": sorted(apis, key=lambda row: row["id"])}


if __name__ == "__main__":
    print(json.dumps(collect(), indent=2))
