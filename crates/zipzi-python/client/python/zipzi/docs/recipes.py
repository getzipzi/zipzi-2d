"""Executable v4 workflows, usable with either Local or Connected Sessions.

Run: python -m zipzi.docs.recipes /absolute/path/new.zipzi
Import individual functions to work with an explicitly selected Editor session.
The CLI creates a new Local document and saves a parameter plus a textured mesh.
"""

from collections.abc import Mapping
import json
from pathlib import Path
import struct
from typing import Any
from uuid import uuid4

import zipzi
from zipzi import values as v

CHUNK = 1 << 20


def read_all(blob: zipzi.Blob) -> bytes:
    """Read owned bulk data without exceeding the per-call transfer bound."""
    chunks: list[bytes] = []
    offset = 0
    while offset < blob.length:
        chunk = blob.read(offset, min(CHUNK, blob.length - offset))
        if not chunk:
            raise ValueError("Unexpected end of Blob")
        chunks.append(chunk)
        offset += len(chunk)
    return b"".join(chunks)


def create_scene(session: zipzi.Session) -> tuple[str, str, str]:
    """Create one parameter and a textured triangle in one undo step; return IDs."""
    parameter, mesh, asset = (str(uuid4()) for _ in range(3))
    points = (v.Vec2(x=0, y=0), v.Vec2(x=1, y=0), v.Vec2(x=0, y=1))
    geometry = v.MeshGeometry(
        vertex_ids=(0, 1, 2),
        positions=points,
        uvs=points,
        triangles=((0, 1, 2),),
    )
    with session.runtime.blob(4) as upload:
        upload.append(0, bytes((255, 0, 0, 255)))  # one red RGBA pixel
        upload.seal()
        with session.draft("Create Angle and triangle") as draft:
            draft.apply(
                v.CommandCreateParameter(
                    spec=v.ParameterCreateSpec(
                        id=parameter,
                        name="Angle",
                        minimum=-30,
                        maximum=30,
                        default_value=0,
                    )
                ),
                v.CommandCreateAsset(id=asset, name="Red pixel"),
                v.CommandCreateMesh(
                    spec=v.MeshCreateSpec(
                        id=mesh,
                        name="Triangle",
                        geometry=geometry,
                        texture_asset_id=asset,
                    )
                ),
            )
            draft.replace_image_blob(asset, upload, width=1, height=1)
    return parameter, mesh, asset


def list_parameters(
    session: zipzi.Session, *, page_size: int = 128
) -> tuple[v.Parameter, ...]:
    """Read every page at one exact source; STALE_SOURCE requires a fresh read."""
    source = session.source
    offset = 0
    parameters: list[v.Parameter] = []
    while True:
        page = session.query(
            v.QueryCollection(
                kind=v.ObjectKind.PARAMETER, offset=offset, limit=page_size
            ),
            source=source,
        )
        parameters.extend(page.records(v.Parameter))
        next_offset = page.value["next_offset"]
        if next_offset is None:
            return tuple(parameters)
        offset = next_offset


def rename_parameter(
    session: zipzi.Session, parameter_id: str, name: str
) -> v.Parameter:
    """Read, rename and verify an explicit ID; refuse concurrent publication."""
    selection = v.QueryObjects(
        addresses=(v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter_id),)
    )
    before = session.query(selection)
    source = before.source
    assert isinstance(source, v.QuerySourcePublished)
    with session.draft("Rename parameter", expected=source.value) as draft:
        draft.apply(v.CommandRename(target=selection.addresses[0], name=name))
        (parameter,) = draft.query(selection).records(v.Parameter)
        assert parameter.name == name
        receipt = draft.publish()
    published = v.QuerySourcePublished(value=v.Version.from_wire(receipt["version"]))
    (parameter,) = session.query(selection, source=published).records(v.Parameter)
    return parameter


def create_linear_motion(session: zipzi.Session, parameter_id: str) -> str:
    """Create a one-second -20..20 parameter sweep; return its clip ID.

    parameter_id must be an existing parameter whose range includes -20..20.
    Clip and track fades are zero so the curve is visible without blending.
    """
    clip_id, track_id, first_key, last_key = (str(uuid4()) for _ in range(4))
    track = v.MotionTrack(
        id=track_id,
        target=v.MotionTrackTargetParameter(parameter_id=parameter_id),
        initial=v.MotionKey(id=first_key, time=0, value=-20),
        segments=(
            v.MotionSegmentLinear(end=v.MotionKey(id=last_key, time=1, value=20)),
        ),
        fade_in=0,
        fade_out=0,
        extensions={},
    )
    clip = v.MotionClip(
        id=clip_id,
        name="Angle sweep",
        duration=1,
        fps=30,
        looping=False,
        restricted_beziers=False,
        fade_in=0,
        fade_out=0,
        track_order=(),
        tracks=(track,),
        events=(),
        extensions={},
        meta_extensions={},
    )
    with session.draft("Create parameter motion") as draft:
        draft.apply(v.CommandCreateMotion(value=clip))
    return clip_id


def sample_animation(
    session: zipzi.Session,
    times: tuple[float, ...],
    *,
    motions: tuple[tuple[str, float], ...] = (),
) -> Mapping[str, Any]:
    """Sample at seconds; motions contains (clip ID, activation time in seconds).

    To schedule activation at the timeline origin, pass ((clip_id, 0.0),).
    Empty motions samples the rest pose. Preview playback activates on a
    canonical update tick; use animation.motion_positions[].local_time when
    comparing the sampled value with a clip curve.
    """
    source = session.source
    request = v.SampleRequest(
        input=v.SampleInputAnimation(times=times, motions=motions)
    )
    with session.samples(request, source=source) as job:
        job.wait(timeout=30)
        with job.blob() as blob:
            result = json.loads(read_all(blob))
    assert result["source"] == source.to_wire()
    return result


def observation_packets(payload: bytes) -> tuple[tuple[Mapping[str, Any], bytes], ...]:
    """Decode observation headers and PNGs; reject incomplete or trailing packets."""
    packets: list[tuple[Mapping[str, Any], bytes]] = []
    offset = 0
    while offset < len(payload):
        if len(payload) - offset < 4:
            raise ValueError("Incomplete observation header length")
        (header_length,) = struct.unpack_from(">I", payload, offset)
        offset += 4
        end = offset + header_length
        if end > len(payload):
            raise ValueError("Incomplete observation header")
        header = json.loads(payload[offset:end])
        size = header["png_bytes"]
        if type(size) is not int or size < 0 or end + size > len(payload):
            raise ValueError("Invalid observation PNG length")
        packets.append((header, payload[end : end + size]))
        offset = end + size
    return tuple(packets)


def observe(session: zipzi.Session) -> tuple[tuple[Mapping[str, Any], bytes], ...]:
    """Render a 64-pixel view and return complete headers and PNG bytes."""
    if "gpu_observation" not in session.runtime.capabilities():
        raise ValueError("This host does not provide GPU observation")
    view = v.ObservationView(width=64, height=64, roi=(-1.0, -1.0, 2.0, 2.0))
    with session.observe(v.SampleInputParameters(values=({},)), view) as job:
        job.wait(timeout=30)
        with job.blob() as blob:
            return observation_packets(read_all(blob))


def create_project(destination: Path) -> None:
    """Create, verify and save a Local scene; print durability warnings."""
    with zipzi.local() as runtime:
        session = runtime.sessions.create()
        parameter, mesh, asset = create_scene(session)
        assert list_parameters(session)[0].id == parameter
        with session.image_blob(asset) as pixels:
            assert read_all(pixels) == bytes((255, 0, 0, 255))
        with session.save(destination) as job:
            receipt = job.wait()
        if not receipt["published"]:
            raise RuntimeError("Project destination was not published")
        print(
            receipt["path"],
            "durable:",
            receipt["durable"],
            "warnings:",
            receipt["warnings"],
        )
        print("parameter:", parameter, "mesh:", mesh)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "destination", type=Path, help="Path for the new native project"
    )
    args = parser.parse_args()
    create_project(args.destination.absolute())
