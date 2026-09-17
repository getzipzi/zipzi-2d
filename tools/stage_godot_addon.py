#!/usr/bin/env python3
"""Stage the canonical gd_cubism addon into Godot projects."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = REPO_ROOT / "modules/gd-cubism/addons/gd_cubism"
EXTENSION_RESOURCE = "res://addons/gd_cubism/gd_cubism.gdextension"
CORE_PROVIDERS = ("cubism", "purism")
PROJECT_PROVIDER_FILE = "gd_cubism_provider.txt"


def select_core_provider(addon_root: Path, core_provider: str) -> None:
    """Select one of the coexisting provider binaries in a staged addon."""
    if core_provider not in CORE_PROVIDERS:
        raise ValueError(f"unknown Core provider: {core_provider}")
    descriptor = addon_root / "gd_cubism.gdextension"
    contents = descriptor.read_text(encoding="utf-8")
    descriptor.write_text(
        contents.replace(".cubism.", f".{core_provider}."),
        encoding="utf-8",
    )


def stage_addon(
    project_root: Path,
    addon_source: Path = DEFAULT_SOURCE,
    *,
    core_provider: str | None = None,
    write_extension_cache: bool = True,
) -> Path:
    project_root = project_root.resolve()
    addon_source = addon_source.resolve()
    if not addon_source.is_dir():
        raise FileNotFoundError(f"addon source does not exist: {addon_source}")
    if not (project_root / "project.godot").is_file():
        raise FileNotFoundError(f"Godot project does not exist: {project_root}")

    provider_file = project_root / PROJECT_PROVIDER_FILE
    if core_provider is None and provider_file.is_file():
        core_provider = provider_file.read_text(encoding="utf-8").strip()

    destination = project_root / "addons/gd_cubism"
    if destination == addon_source:
        raise ValueError("addon source and staging destination must be different")
    if destination.exists():
        shutil.rmtree(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(addon_source, destination)
    if core_provider is not None:
        select_core_provider(destination, core_provider)

    if write_extension_cache:
        extension_cache = project_root / ".godot/extension_list.cfg"
        extension_cache.parent.mkdir(parents=True, exist_ok=True)
        existing = (
            extension_cache.read_text(encoding="utf-8").splitlines()
            if extension_cache.exists()
            else []
        )
        entries = [
            line
            for line in existing
            if "addons/gd_cubism/" not in line
        ]
        entries.append(EXTENSION_RESOURCE)
        extension_cache.write_text("\n".join(entries) + "\n", encoding="utf-8")

    print(f"staged {addon_source} -> {destination}")
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("projects", nargs="+", type=Path, help="Godot project roots")
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--core-provider", choices=CORE_PROVIDERS)
    parser.add_argument("--no-extension-cache", action="store_true")
    args = parser.parse_args()

    try:
        for project in args.projects:
            stage_addon(
                project,
                args.source,
                core_provider=args.core_provider,
                write_extension_cache=not args.no_extension_cache,
            )
    except (FileNotFoundError, ValueError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
