"""Edit an existing Editor document through its shared Session and history.

Run ``python -I -m zipzi.docs.editor_handoff list`` against a running Editor,
then ``rename --runtime UUID --session SERIAL --mesh UUID --name NAME``.
Use --endpoint /absolute/path/to/profile.sock before the subcommand when needed.
The selected Session stays resident after the client exits; saving is separate.
"""
from __future__ import annotations

import argparse
from pathlib import Path

import zipzi


def rename_mesh(session: zipzi.Session, mesh_id: str, name: str) -> zipzi.Receipt:
    """Read the latest candidate and publish one rename into the same history.

    Busy, stale versions and OutcomeUnknown use the ordinary Edit/Job contract.
    Recover an unknown original call before submitting further work. A no-op
    follows SDK history rules and preserves redo. No file is saved implicitly.
    """
    with session.edit("Editor handoff: rename mesh") as edit:
        current = edit.meshes.get(mesh_id)
        edit.meshes.rename(mesh_id, name, expected_name=current.value.name)
        receipt = edit.commit().wait()
    return receipt


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", type=Path, help="Existing Editor socket (absolute path)")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list", help="List resident Session identities and metadata")
    rename = commands.add_parser("rename", help="Rename one mesh in an explicitly selected Session")
    rename.add_argument("--runtime", required=True, help="Runtime UUID from list")
    rename.add_argument("--session", required=True, type=int, help="Session serial from list")
    rename.add_argument("--mesh", required=True, help="Mesh UUID in the selected document")
    rename.add_argument("--name", required=True, help="New mesh name")
    args = parser.parse_args()
    with zipzi.connect(endpoint=args.endpoint) as runtime:
        if args.command == "list":
            for info in runtime.sessions.list():
                print(info.id, info.summary())
        else:
            session = runtime.sessions.attach(zipzi.SessionId(args.runtime, args.session))
            receipt = rename_mesh(session, args.mesh, args.name)
            print(receipt.summary())


if __name__ == "__main__":
    main()
