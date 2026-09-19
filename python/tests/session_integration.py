"""Exercise the real Godot session from a separate Python process."""
from __future__ import annotations

import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

from kasane_sdk import KasaneClient, KasaneError


ROOT = Path(__file__).resolve().parents[2]
DEMO = ROOT / "demos/kasane-preview"
GODOT = os.environ.get("GODOT_BIN", "/Applications/Godot_mono.app/Contents/MacOS/Godot")


def expect_error(code: str, action) -> None:
    try:
        action()
    except KasaneError as error:
        assert error.code == code, (code, error.code)
    else:
        raise AssertionError(f"Expected {code}")


def check_session(port: int) -> None:
    with KasaneClient(port=port) as client:
        summary = client.summary()
        assert len(summary["meshes"]) == 1
        mesh_id = summary["meshes"][0]["id"]
        source = client.get_mesh(mesh_id)
        assert source["vertex_ids"] == [40, 10, 90, 20]
        asset = client.get_asset(source["texture_asset_id"])
        assert asset["width"] == 32 and asset["height"] == 32
        expect_error("MISSING_MESH", lambda: client.get_mesh("missing"))

        start = summary["revision"]
        client.begin(start)
        client.stage_positions(mesh_id, [40], [[-101, 111]])
        client.stage_positions(mesh_id, [20], [[115, 103]])
        assert client.get_mesh(mesh_id)["base_positions"] == source["base_positions"]
        committed = client.commit()
        assert committed["revision"] == start + 1
        edited = client.get_mesh(mesh_id)
        assert edited["base_positions"][0] == [-101, 111]
        assert edited["base_positions"][3] == [115, 103]
        assert client.preview(mesh_id)["positions"] == edited["base_positions"]
        client.undo()
        assert client.get_mesh(mesh_id)["base_positions"] == source["base_positions"]
        client.redo()
        assert client.get_mesh(mesh_id)["base_positions"] == edited["base_positions"]

        stale = start
        client.begin(stale)
        client.stage_positions(mesh_id, [40], [[0, 0]])
        expect_error("STALE_REVISION", client.commit)
        assert client.get_mesh(mesh_id)["base_positions"] == edited["base_positions"]

        client.begin(client.summary()["revision"])
        client.stage_positions(mesh_id, [40], [[0, 0]])
        client.stage_positions(mesh_id, [999], [[1, 1]])
        expect_error("MISSING_VERTEX", client.commit)
        assert client.get_mesh(mesh_id)["base_positions"] == edited["base_positions"]

        try:
            with client.transaction():
                client.stage_positions(mesh_id, [40], [[7, 8]])
                raise ValueError("script calculation failed")
        except ValueError:
            pass
        assert client.get_mesh(mesh_id)["base_positions"] == edited["base_positions"]

        grid = client.create_grid("Python grid", asset["id"], 2, 2, 64, 64)
        assert grid["ok"]
        grid_id = grid["changed_meshes"][0]
        geometry = client.get_mesh(grid_id)
        assert len(geometry["vertex_ids"]) == 9 and len(geometry["triangles"]) == 24
        assert client.preview(grid_id)["positions"] == geometry["base_positions"]
        with client.transaction():
            client.stage_positions(grid_id, [0, 8], [[-40, 40], [40, -40]])
        assert client.preview(grid_id)["positions"][0] == [-40, 40]
        expect_error("STALE_REVISION", lambda: client.create_grid("stale", asset["id"], 1, 1, 20, 20, revision=start))

        before_disconnect = client.get_mesh(mesh_id)["base_positions"]
        client.begin(client.summary()["revision"])
        client.stage_positions(mesh_id, [40], [[333, 444]])
    # A dropped socket discards only its local draft.
    time.sleep(0.1)
    with KasaneClient(port=port) as client:
        assert client.get_mesh(mesh_id)["base_positions"] == before_disconnect
        assert not client.summary()["transaction_active"]
    print("KASANE_PYTHON_SESSION_TEST_OK")


def main() -> None:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    env = os.environ.copy()
    env["KASANE_EDIT_PORT"] = str(port)
    with tempfile.TemporaryFile(mode="w+t") as log:
        process = subprocess.Popen([GODOT, "--headless", "--path", str(DEMO)], stdout=log,
                                   stderr=subprocess.STDOUT, env=env)
        try:
            deadline = time.monotonic() + 15
            while True:
                if process.poll() is not None:
                    raise RuntimeError("Godot exited before the session was ready")
                try:
                    with KasaneClient(port=port, timeout=0.5) as client:
                        client.summary()
                    break
                except (OSError, ConnectionError):
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Kasane session did not start")
                    time.sleep(0.1)
            check_session(port)
        except Exception:
            log.seek(0)
            print(log.read(), file=sys.stderr)
            raise
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    main()
