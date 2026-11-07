"""Bounded offline help derived from the installed public Python objects."""

from dataclasses import dataclass
from collections.abc import Mapping
from enum import Enum
import inspect
import sys
from types import ModuleType
from typing import Any, TypeAliasType, cast, is_typeddict


@dataclass(frozen=True)
class _Field:
    annotation: Any


def _members(target: Any) -> Mapping[str, Any]:
    if isinstance(target, type):
        members = {
            name: member
            for base in reversed(target.__mro__)
            for name, member in vars(base).items()
        }
        # Expose typed instance namespaces without constructing a Runtime.
        module = vars(sys.modules[target.__module__])
        for name, annotation in inspect.get_annotations(target).items():
            member = (
                module.get(annotation) if isinstance(annotation, str) else annotation
            )
            if name not in members:
                members[name] = (
                    member if isinstance(member, type) else _Field(annotation)
                )
        return members
    if isinstance(target, ModuleType):
        return vars(target)
    return {}


@dataclass(frozen=True)
class ApiDescription:
    """One help entry and a page of child paths; format() performs no host IO."""

    path: str
    signature: str
    documentation: str
    children: tuple[str, ...]
    next_offset: int | None

    def format(self) -> str:
        """Render this entry without expanding children or referenced types."""
        lines = [self.path + self.signature, "", self.documentation]
        if self.children:
            lines += ["", "Children:", *self.children]
        if self.next_offset is not None:
            lines += [
                "",
                f"More children: describe({self.path!r}, offset={self.next_offset})",
            ]
        return "\n".join(lines)


def describe(
    path: str = "zipzi", *, offset: int = 0, limit: int = 32
) -> ApiDescription:
    """Read installed help without a host, native backend or network.

    Paths include Session.query, Snapshot.records, results.SaveReceipt and
    values.CommandCreateParameter. Enumerate values or a union's subclasses
    using children; next_offset continues the same entry. Each page has at most
    64 children. Ordinary help(object) and inspect.signature(object) also work.
    """
    import zipzi

    if not isinstance(path, str):
        raise TypeError("Expected a public API path")
    if (
        type(offset) is not int
        or offset < 0
        or type(limit) is not int
        or not 1 <= limit <= 64
    ):
        raise ValueError("Expected offset >= 0 and limit in 1..64")
    parts = path.split(".")
    if parts[0] != "zipzi":
        parts.insert(0, "zipzi")
    target: Any = zipzi
    visited = ["zipzi"]
    for part in parts[1:]:
        if not part or part.startswith("_"):
            raise KeyError(
                f"Unknown public API path {path!r}; use describe({'.'.join(visited)!r}).format() to list public children."
            )
        namespace = _members(target)
        allowed: Any = namespace.get("__all__")
        if part not in namespace or (allowed is not None and part not in allowed):
            raise KeyError(
                f"Unknown public API path {path!r}; use describe({'.'.join(visited)!r}).format() to list public children."
            )
        target = namespace[part]
        visited.append(part)
    canonical = ".".join(parts)
    namespace = _members(target)
    if isinstance(target, ModuleType):
        names = list(namespace.get("__all__", ()))
    elif isinstance(target, type) and issubclass(target, Enum):
        names = list(target.__members__)
    elif is_typeddict(cast(Any, target)):
        # Responses are immutable mappings, so dict mutation methods are not API entries.
        names = []
    elif isinstance(target, type):
        names = [
            name
            for name, member in namespace.items()
            if not name.startswith("_")
            and (
                callable(member) or isinstance(member, (property, classmethod, _Field))
            )
        ]
        # Sum types expose concrete constructors without a parallel API catalog.
        names += [
            name
            for name in zipzi.values.__all__
            if isinstance(child := vars(zipzi.values)[name], type)
            and target in child.__bases__
        ]
    else:
        names = []
    children = sorted(
        {
            (
                "zipzi.values." + name
                if name not in namespace
                else canonical + "." + name
            )
            for name in names
        }
    )
    target = cast(Any, target)
    try:
        signature = str(inspect.signature(target)) if callable(target) else ""
    except (TypeError, ValueError):
        signature = ""
    # A classmethod descriptor is resolved only after lookup in an allowed namespace.
    if isinstance(target, classmethod):
        target = cast(Any, target).__func__
        signature = str(inspect.signature(target))
    documentation = (
        inspect.getdoc(target) or "See the installed guide and related value types."
    )
    if isinstance(target, Enum):
        documentation = f"Value: {target.value!r}\n\n" + documentation
    elif isinstance(target, TypeAliasType):
        documentation = f"{target.__name__} = {target.__value__}"
    elif isinstance(target, _Field):
        documentation = f"Instance attribute. Type: {target.annotation}"
    if isinstance(target, type) and "__annotations__" in namespace:
        annotations = inspect.get_annotations(target)
        if annotations:
            documentation += "\n\nField types:\n" + "\n".join(
                f"{name}: {annotation}"
                for name, annotation in annotations.items()
                if not name.startswith("_")
            )
    end = offset + limit
    return ApiDescription(
        canonical,
        signature,
        documentation,
        tuple(children[offset:end]),
        end if end < len(children) else None,
    )
