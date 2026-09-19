"""Query the open quad, move two stable vertex IDs, and print the result."""
from __future__ import annotations

import os
from kasane_sdk import KasaneClient


with KasaneClient(port=int(os.environ.get("KASANE_EDIT_PORT", "43884"))) as client:
    summary = client.summary()
    mesh_id = summary["meshes"][0]["id"]
    mesh = client.get_mesh(mesh_id)
    print("Before:", mesh["revision"], dict(zip(mesh["vertex_ids"], mesh["base_positions"])))
    with client.transaction(summary["revision"]):
        client.stage_positions(mesh_id, [40, 20], [[-105.0, 110.0], [120.0, 95.0]])
    mesh = client.get_mesh(mesh_id)
    print("After:", mesh["revision"], dict(zip(mesh["vertex_ids"], mesh["base_positions"])))
    print("Preview:", client.preview(mesh_id)["positions"])
