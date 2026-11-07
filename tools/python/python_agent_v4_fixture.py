"""Prepare and independently verify v4 installed-wheel agent tasks.

This is a new diagnostic suite, separate from the frozen historical P3A cohort.
Run this controller with a normal installed zipzi + zipzi-native interpreter.
The participant receives task.json and input files; expectations and observations
stay in the adjacent private directory. Agent declarations never decide a pass.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import math
from pathlib import Path
from uuid import NAMESPACE_URL, uuid5

VERSION = "v4-installed-agent-1"
TASKS = ("A01", "A02", "A03", "A04")
CHUNK = 1 << 20


def uid(task: str, label: str) -> str:
    return str(uuid5(NAMESPACE_URL, f"zipzi/{VERSION}/{task}/{label}"))


def plain(value):
    if hasattr(value, "to_wire"):
        return value.to_wire()
    if isinstance(value, dict) or hasattr(value, "items"):
        return {str(key): plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(item) for item in value]
    return value


def write_json(path: Path, value) -> None:
    path.write_text(
        json.dumps(plain(value), indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def asset_metadata_matches(asset: dict, data: dict) -> bool:
    """A byte digest alone cannot establish an image's identity or dimensions."""
    content = asset.get("content", {})
    return (
        asset.get("id") == data["asset_id"]
        and asset.get("name") == "Agent texture"
        and content.get("kind") == "image"
        and content.get("width") == data["width"]
        and content.get("height") == data["height"]
    )


def read_blob(blob) -> bytes:
    return b"".join(
        blob.read(offset, min(CHUNK, blob.length - offset))
        for offset in range(0, blob.length, CHUNK)
    )


def records(session, kind) -> list[dict]:
    from zipzi import values as v

    source = session.source
    offset, result = 0, []
    while True:
        page = session.query(
            v.QueryCollection(kind=kind, offset=offset, limit=128), source=source
        )
        result.extend(plain(page.value["records"]))
        offset = page.value["next_offset"]
        if offset is None:
            return result


@contextmanager
def opened(path: Path):
    import zipzi

    with zipzi.local() as runtime:
        session = runtime.sessions.create()
        with session.open(path) as job:
            job.wait(timeout=30)
        yield session


def setup(
    packet: Path,
    private: Path,
    task_id: str,
    environment: str,
    endpoint: str | None = None,
) -> dict:
    import zipzi
    from zipzi import values as v

    if task_id not in TASKS or environment not in ("local", "connected"):
        raise ValueError("Unknown task or environment")
    if environment == "connected" and endpoint is None:
        raise ValueError("Connected setup requires an endpoint")
    packet, private = packet.resolve(), private.resolve()
    if private == packet or private.is_relative_to(packet):
        raise ValueError("Private controller state must be outside participant packet")
    for directory in (packet / "input", packet / "output", packet / "work", private):
        directory.mkdir(parents=True, exist_ok=True)
    if (packet / "task.json").exists() or (private / "expected.json").exists():
        raise ValueError("Task fixtures are immutable; select a fresh packet")
    data = {
        "suite": VERSION,
        "task_id": task_id,
        "environment": environment,
        "endpoint": endpoint,
        "input_project": str(packet / "input/model.zipzi"),
        "output_project": str(packet / "output/result.zipzi"),
        "output_directory": str(packet / "output"),
        "document_id": uid(task_id, "document"),
    }
    count = 300 if task_id == "A01" else 3
    with (
        zipzi.local()
        if environment == "local"
        else zipzi.connect(endpoint=Path(endpoint))
    ) as runtime:
        session = runtime.sessions.create(document_id=data["document_id"])
        baseline = []
        for start in range(0, count, 64):
            with session.draft("Prepare independent fixture") as draft:
                for index in range(start, min(count, start + 64)):
                    parameter_id = uid(task_id, f"parameter-{index}")
                    draft.apply(
                        v.CommandCreateParameter(
                            spec=v.ParameterCreateSpec(
                                id=parameter_id,
                                name="Angle"
                                if index % 17 == 0
                                else f"Parameter {index}",
                                minimum=-30,
                                maximum=30,
                                default_value=0,
                            )
                        )
                    )
        baseline = records(session, v.ObjectKind.PARAMETER)
        baseline_domains = {
            kind.value: records(session, kind)
            for kind in v.ObjectKind
            if kind != v.ObjectKind.PARAMETER
        }
        with session.save(Path(data["input_project"])) as job:
            job.wait(timeout=30)
        data["session_id"] = session.id if environment == "connected" else None
        if environment == "connected":
            decoy = runtime.sessions.create(document_id=uid(task_id, "decoy-document"))
            with decoy.draft("Decoy") as draft:
                draft.apply(
                    v.CommandCreateParameter(
                        spec=v.ParameterCreateSpec(
                            id=uid(task_id, "decoy-parameter"),
                            name="Angle",
                            minimum=-1,
                            maximum=1,
                            default_value=0,
                        )
                    )
                )
            data["decoy_session_id"] = decoy.id
        target_index = 289 if task_id == "A01" else 0
        data["parameter_id"] = uid(task_id, f"parameter-{target_index}")
        expected = {
            "suite": VERSION,
            "task_id": task_id,
            "baseline_parameters": baseline,
            "initial_history": plain(session.info()["history"]),
            "document_id": data["document_id"],
            "setup": plain(session.query(v.QuerySummary()).value["setup"]),
            "baseline_domains": baseline_domains,
        }
        if environment == "connected":
            expected["decoy_parameters"] = records(decoy, v.ObjectKind.PARAMETER)
            expected["decoy_domains"] = {
                kind.value: records(decoy, kind)
                for kind in v.ObjectKind
                if kind != v.ObjectKind.PARAMETER
            }
        if task_id == "A01":
            data.update(
                expected_parameter_count=count, new_name="Head angle", new_default=12.5
            )
            data["goal"] = (
                "Select the document with document_id, not the decoy. Enumerate all 300 parameter records "
                "consistently even though a collection page cannot contain them all, and write the complete "
                "ID list to output/inventory.json. Duplicate names are intentional. Change only parameter_id: "
                "rename it to new_name and set default_value to new_default while preserving every other "
                "field and object. Save output_project and verify its saved publication/durability receipt. "
                "Do not recreate the model from a list or modify another resident document."
            )
        elif task_id == "A02":
            width, height = 768, 512
            red, blue = (
                bytes((220, 30, 10, 255)) * (width * height),
                bytes((10, 30, 220, 255)) * (width * height),
            )
            (packet / "input/red.rgba").write_bytes(red)
            (packet / "input/blue.rgba").write_bytes(blue)
            data.update(
                width=width,
                height=height,
                red_pixels=str(packet / "input/red.rgba"),
                blue_pixels=str(packet / "input/blue.rgba"),
                asset_id=uid(task_id, "asset"),
                mesh_id=uid(task_id, "mesh"),
                vertex_ids=[7, 19, 31],
                positions=[[0, 0], [2, 0], [0, 2]],
                uvs=[[0, 0], [1, 0], [0, 1]],
            )
            expected.update(red_sha256=digest(red), blue_sha256=digest(blue))
            data["goal"] = (
                "Preserve the existing model. Create one asset asset_id named Agent texture and one "
                "mesh mesh_id named Agent triangle, using vertex_ids, positions and uvs; its single "
                "triangle uses the three supplied IDs and references asset_id. Upload red_pixels as "
                "width-by-height RGBA using the bulk API and publish it with the mesh. In a second edit "
                "replace it with blue_pixels using bulk bytes. Undo that second edit and save "
                "output/undo.zipzi with red pixels, then Redo and save output_project with blue pixels. "
                "Read back all final pixels through bounded bulk reads and write their SHA-256 to "
                "output/pixels.sha256. Verify actual readback at both Undo and Redo, preserve existing "
                "parameters, and release Jobs and Blobs."
            )
        elif task_id == "A03":
            data.update(
                clip_id=uid(task_id, "clip"),
                track_id=uid(task_id, "track"),
                start_key_id=uid(task_id, "key-start"),
                end_key_id=uid(task_id, "key-end"),
                times=[0.0, 0.25, 0.5, 0.75, 1.0],
                playback_step_seconds=1 / 60,
                key_values=[-20.0, 20.0],
            )
            # Independent arithmetic from the documented fixed-step activation
            # contract; these targets are never generated by executing Runtime.
            data["expected_clip_local_times"] = [
                max(0.0, time - 1 / 60) for time in data["times"]
            ]
            data["values"] = [
                -20 + 40 * local for local in data["expected_clip_local_times"]
            ]
            expected["initial_specification_discrepancy"] = {
                "ideal_absolute_time_curve_values": [-20.0, -10.0, 0.0, 10.0, 20.0],
                "reason": "Combined playback activates at the first canonical 1/60-second update; it is not raw clip-local curve evaluation.",
                "independent_rule": "local_time=max(0, requested_time-1/60); value=-20+40*local_time",
            }
            data["goal"] = (
                "Preserve existing parameters. Create a motion clip clip_id named Agent linear, duration "
                "1 second, 30 FPS, not looping and zero fades, containing track_id targeting parameter_id. "
                "Its initial key start_key_id is (0 seconds, -20) and its linear segment ends at "
                "end_key_id (1 second, 20). Start this clip at absolute preview time zero and sample actual "
                "combined preview playback at times. Distinguish requested preview-clock time from reported "
                "clip-local time: this playback clock activates on its first 1/60-second update. Check the "
                "reported clip-local times against expected_clip_local_times and the linear curve's values "
                "at those local times against values. This task requests playback, not raw curve evaluation. "
                "Write the complete actual sample JSON "
                "to output/samples.json. Demonstrate that sampling leaves the publication version and "
                "undo history unchanged by recording session info before and after it. Save output_project "
                "and verify durability warnings. Avoid changing the authored default parameter value."
            )
        else:
            data.update(
                anchor_id=uid(task_id, "anchor"),
                recovered_id=uid(task_id, "recovered"),
                discarded_id=uid(task_id, "discarded"),
            )
            data["goal"] = (
                "Preserve existing parameters. Capture a query source, then publish a new parameter "
                "anchor_id named Published anchor with range -1..1 and default 0. Attempt an actual query "
                "using the old source and handle the stale-source rejection by making a fresh query. "
                "In another Draft, stage discarded_id named Must not leak, then submit an ordinary invalid "
                "mutation (such as creating an existing parameter_id); confirm the failed Draft ended and "
                "none of its candidate content was published. Start a fresh Draft and asynchronously create "
                "recovered_id named Recovered with range -1..1 and default 0. Call wait(timeout=0); it may "
                "already be finished, or may raise JobTimeout. In the latter case retain ownership and wait "
                "again inside the Job context. Publish only after the job resolves. Save output_project. "
                "The final model must contain just the three original parameters plus Published anchor and "
                "Recovered, never Must not leak. Actually exercise these operations; declarations in a "
                "report alone do not satisfy them. Verify the saved receipt and release all resources."
            )
        write_json(private / "expected.json", expected)
        write_json(packet / "task.json", data)
    return data


def ledger_events(private: Path) -> list[dict]:
    path = private / "observations.ndjson"
    return (
        [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
        if path.exists()
        else []
    )


def _code(event):
    error = event.get("error")
    return error.get("code") if isinstance(error, dict) else event.get("code")


def _argument(event, name):
    args = event.get("arguments", event.get("kwargs", {}))
    if isinstance(args, dict):
        return args.get(name, args.get("kwargs", {}).get(name))
    return None


def _result(event):
    return event.get("result", event.get("value"))


def _readback(event):
    # New observers assemble successful reads by Blob ID and byte offset. Keep
    # historical single-read evidence inspectable without rewriting old trials.
    return event.get("readback", _result(event))


def _byte_lengths(value):
    if isinstance(value, dict):
        if "length" in value and "sha256" in value:
            return [value["length"]]
        return [length for item in value.values() for length in _byte_lengths(item)]
    if isinstance(value, (tuple, list)):
        return [length for item in value for length in _byte_lengths(item)]
    return []


def _sample_values(report, parameter_id: str) -> list[float]:
    return [
        frame["animation"]["parameters"][parameter_id] for frame in report["frames"]
    ]


def playback_matches(report: dict, data: dict) -> bool:
    """Check playback against independent clock math and the stored linear curve."""
    frames = report.get("frames", [])
    if len(frames) != len(data["times"]):
        return False
    try:
        for requested, frame in zip(data["times"], frames):
            animation = frame["animation"]
            positions = [
                position
                for position in animation["motion_positions"]
                if position["motion_id"] == data["clip_id"]
            ]
            if len(positions) != 1:
                return False
            local = positions[0]["local_time"]
            canonical = max(0.0, requested - 1 / 60)
            if not (
                math.isclose(animation["time"], requested, abs_tol=1e-5)
                and math.isclose(local, canonical, abs_tol=1e-5)
                and math.isclose(
                    animation["parameters"][data["parameter_id"]],
                    -20 + 40 * local,
                    abs_tol=1e-5,
                )
            ):
                return False
        return True
    except (KeyError, TypeError):
        return False


def observation_checks(task: str, data: dict, events: list[dict]) -> dict[str, bool]:
    """Transient requirements come from trusted public-call observations only."""
    checks: dict[str, bool] = {}
    success = lambda path: [
        e
        for e in events
        if e.get("path") == path and not e.get("error") and _result(e) is not None
    ]
    if task == "A01":
        pages = []
        for event in success("Session.query"):
            result = _result(event)
            if (
                isinstance(result, dict)
                and isinstance(result.get("value"), dict)
                and "next_offset" in result["value"]
            ):
                pages.append(result)
        groups = {}
        for page in pages:
            source = json.dumps(page["source"], sort_keys=True)
            groups.setdefault(source, []).extend(
                item["address"]["id"] for item in page["value"]["records"]
            )
        checks["observed_consistent_complete_scan"] = any(
            len(set(ids)) == data["expected_parameter_count"] for ids in groups.values()
        )
    elif task == "A02":
        # Undo/Redo return publication receipts. Bulk bytes are represented by the
        # observer as {length, sha256}; they never enter the control log verbatim.
        checks["observed_undo"] = bool(success("Session.undo"))
        checks["observed_redo"] = bool(success("Session.redo"))
        uploads, downloads = [], []
        for event in success("Blob.append"):
            args = event.get("arguments", event.get("args", {}))
            uploads.extend(_byte_lengths(args))
        for event in success("Blob.read"):
            value = _result(event)
            if isinstance(value, dict) and "length" in value:
                downloads.append(value["length"])
        checks["observed_bounded_multi_chunk_upload"] = (
            bool(uploads)
            and max(uploads) <= CHUNK
            and sum(uploads) >= data["width"] * data["height"] * 8
        )
        checks["observed_bounded_multi_chunk_readback"] = (
            bool(downloads)
            and max(downloads) <= CHUNK
            and sum(downloads) >= data["width"] * data["height"] * 8
        )
    elif task == "A03":
        reports = []
        for event in success("Blob.read"):
            value = _readback(event)
            if isinstance(value, dict) and isinstance(value.get("json"), dict):
                reports.append(value["json"])
        checks["observed_real_sample_values"] = any(
            playback_matches(report, data) for report in reports
        )
        infos = [
            (index, _result(event))
            for index, event in enumerate(events)
            if event.get("path") == "Session.info"
            and not event.get("error")
            and isinstance(_result(event), dict)
        ]
        sample_calls = [
            index
            for index, event in enumerate(events)
            if event.get("path") == "Session.samples"
        ]
        reads = [
            index
            for index, event in enumerate(events)
            if event.get("path") == "Blob.read"
            and not event.get("error")
            and isinstance(_readback(event), dict)
            and isinstance(_readback(event).get("json"), dict)
            and "frames" in _readback(event)["json"]
        ]
        checks["observed_read_only_sampling"] = any(
            before < sample < read < after
            and first["version"] == last["version"]
            and first["history"] == last["history"]
            for before, first in infos
            for after, last in infos
            for sample in sample_calls
            for read in reads
        )
    elif task == "A04":
        checks["observed_stale_source_error"] = any(
            e.get("path") == "Session.query" and _code(e) == "STALE_SOURCE"
            for e in events
        )
        checks["observed_ordinary_failed_draft_ended"] = any(
            e.get("path") == "Draft.apply"
            and _code(e) not in (None, "DRAFT_BUSY", "PROJECT_BUSY", "STALE_SOURCE")
            and e.get("state", e.get("draft_state")) == "ended"
            for e in events
        )
        checks["observed_zero_timeout_wait"] = False
        checks["observed_async_completion"] = False
        for start, event in enumerate(events):
            if event.get("path") != "Draft.apply_async" or event.get("error"):
                continue
            job, source = _result(event), event.get("source")
            commands = _argument(event, "commands") or []
            if not (
                isinstance(job, dict)
                and job.get("id")
                and isinstance(source, dict)
                and source.get("draft")
                and any(
                    c.get("command") == "create_parameter"
                    and c.get("spec", {}).get("id") == data.get("recovered_id")
                    for c in commands
                )
            ):
                continue
            zero_waited = False
            for finish in range(start + 1, len(events)):
                wait = events[finish]
                if wait.get("path") != "Job.wait" or wait.get("owner") != job["id"]:
                    continue
                if _argument(wait, "timeout") == 0 and _code(wait) in (
                    None,
                    "JOB_TIMEOUT",
                ):
                    zero_waited = True
                    checks["observed_zero_timeout_wait"] = True
                completed = _result(wait)
                if (
                    not zero_waited
                    or wait.get("error")
                    or not isinstance(completed, dict)
                    or completed.get("draft") != source["draft"]
                    or completed.get("base") != source.get("base")
                    or completed.get("sequence", -1) <= source.get("sequence", -1)
                ):
                    continue
                checks["observed_async_completion"] |= any(
                    publication.get("path") == "Draft.publish"
                    and not publication.get("error")
                    and isinstance(_result(publication), dict)
                    and publication.get("state") == "ended"
                    and publication.get("source", {}).get("draft") == completed["draft"]
                    and publication.get("source", {}).get("base") == completed["base"]
                    and publication.get("source", {}).get("sequence", -1)
                    >= completed["sequence"]
                    for publication in events[finish + 1 :]
                )
    return checks


def verify(packet: Path, private: Path) -> dict:
    from zipzi import values as v

    packet, private = packet.resolve(), private.resolve()
    data, expected = (
        read_json(packet / "task.json"),
        read_json(private / "expected.json"),
    )
    checks, error = {}, None
    try:
        output = Path(data["output_project"])
        checks["output_exists"] = output.is_file()
        if not output.is_file():
            raise ValueError("Required result.zipzi is absent")
        with opened(output) as session:
            checks["document_identity_preserved"] = (
                session.info()["document_id"] == expected["document_id"]
            )
            checks["model_setup_preserved"] = (
                plain(session.query(v.QuerySummary()).value["setup"])
                == expected["setup"]
            )
            actual = records(session, v.ObjectKind.PARAMETER)
            wanted = expected["baseline_parameters"]
            if data["task_id"] == "A01":
                for item in wanted:
                    if item["address"]["id"] == data["parameter_id"]:
                        item["value"].update(
                            name=data["new_name"], default_value=data["new_default"]
                        )
                inventory = read_json(packet / "output/inventory.json")
                if isinstance(inventory, dict):
                    inventory = inventory.get("ids", inventory.get("parameter_ids"))
                checks["inventory_complete"] = (
                    isinstance(inventory, list)
                    and len(inventory) == len(wanted)
                    and set(inventory) == {r["address"]["id"] for r in wanted}
                )
            elif data["task_id"] == "A04":
                baseline = {item["address"]["id"]: item for item in wanted}
                actual_by_id = {item["address"]["id"]: item for item in actual}
                checks["baseline_preserved"] = all(
                    actual_by_id.get(key) == value for key, value in baseline.items()
                )
                checks["only_successful_parameters_exist"] = set(actual_by_id) == set(
                    baseline
                ) | {data["anchor_id"], data["recovered_id"]}
                checks["new_parameter_values"] = all(
                    actual_by_id.get(identity, {}).get("value", {}).get("name") == name
                    and actual_by_id.get(identity, {}).get("value", {}).get("minimum")
                    == -1
                    and actual_by_id.get(identity, {}).get("value", {}).get("maximum")
                    == 1
                    and actual_by_id.get(identity, {})
                    .get("value", {})
                    .get("default_value")
                    == 0
                    for identity, name in (
                        (data["anchor_id"], "Published anchor"),
                        (data["recovered_id"], "Recovered"),
                    )
                )
                wanted = (
                    actual
                    if checks["baseline_preserved"]
                    and checks["only_successful_parameters_exist"]
                    else wanted
                )
            checks["parameters_exact"] = actual == wanted
            actual_domains = {
                kind.value: records(session, kind)
                for kind in v.ObjectKind
                if kind != v.ObjectKind.PARAMETER
            }
            allowed = {
                "A01": set(),
                "A04": set(),
                "A02": {v.ObjectKind.MESH.value, v.ObjectKind.ASSET.value},
                "A03": {
                    v.ObjectKind.MOTION.value,
                    v.ObjectKind.MOTION_TRACK.value,
                    v.ObjectKind.MOTION_KEY.value,
                    v.ObjectKind.MOTION_REGISTRATION.value,
                },
            }[data["task_id"]]
            checks["unrelated_domains_unchanged"] = all(
                items == expected["baseline_domains"][kind]
                for kind, items in actual_domains.items()
                if kind not in allowed
            )
            if data["task_id"] == "A02":
                meshes, assets = (
                    records(session, v.ObjectKind.MESH),
                    records(session, v.ObjectKind.ASSET),
                )
                checks["exactly_one_mesh_and_asset"] = len(meshes) == len(assets) == 1
                checks["asset_identity_name_and_dimensions"] = len(
                    assets
                ) == 1 and asset_metadata_matches(assets[0]["value"], data)
                mesh = v.Mesh.from_wire(meshes[0]["value"])
                checks["mesh_identity_geometry"] = (
                    mesh.id == data["mesh_id"]
                    and mesh.name == "Agent triangle"
                    and list(mesh.vertex_ids) == data["vertex_ids"]
                    and plain(mesh.triangles) == [data["vertex_ids"]]
                    and [[p.x, p.y] for p in mesh.base_positions] == data["positions"]
                    and [[p.x, p.y] for p in mesh.uvs] == data["uvs"]
                    and isinstance(mesh.texture_asset_id, v.SceneReferenceResolved)
                    and mesh.texture_asset_id.object_id == data["asset_id"]
                )
                with session.image_blob(data["asset_id"]) as image:
                    checks["final_pixels_blue"] = (
                        digest(read_blob(image)) == expected["blue_sha256"]
                    )
                checks["readback_digest_matches"] = (
                    packet / "output/pixels.sha256"
                ).read_text().strip().split()[0] == expected["blue_sha256"]
                with opened(packet / "output/undo.zipzi") as undone:
                    with undone.image_blob(data["asset_id"]) as image:
                        checks["undo_pixels_red"] = (
                            digest(read_blob(image)) == expected["red_sha256"]
                        )
            elif data["task_id"] == "A03":
                clips = records(session, v.ObjectKind.MOTION)
                checks["one_expected_clip"] = (
                    len(clips) == 1 and clips[0]["address"]["id"] == data["clip_id"]
                )
                clip = v.MotionClip.from_wire(clips[0]["value"])
                track = clip.tracks[0]
                checks["motion_structure"] = (
                    clip.name == "Agent linear"
                    and clip.duration == 1
                    and clip.fps == 30
                    and not clip.looping
                    and clip.fade_in == clip.fade_out == 0
                    and len(clip.tracks) == 1
                    and track.id == data["track_id"]
                    and isinstance(track.target, v.MotionTrackTargetParameter)
                    and track.target.parameter_id == data["parameter_id"]
                    and track.initial.id == data["start_key_id"]
                    and track.initial.time == 0
                    and track.initial.value == -20
                    and len(track.segments) == 1
                    and isinstance(track.segments[0], v.MotionSegmentLinear)
                    and track.segments[0].end.id == data["end_key_id"]
                    and track.segments[0].end.time == 1
                    and track.segments[0].end.value == 20
                )
                before = plain(session.info())
                request = v.SampleRequest(
                    input=v.SampleInputAnimation(
                        times=tuple(data["times"]), motions=((data["clip_id"], 0.0),)
                    )
                )
                with session.samples(request) as job:
                    job.wait(timeout=30)
                    with job.blob() as blob:
                        report = json.loads(read_blob(blob))
                after = plain(session.info())
                checks["independent_samples_match"] = playback_matches(report, data)
                checks["independent_sampling_read_only"] = (
                    before["version"] == after["version"]
                    and before["history"] == after["history"]
                )
                saved = read_json(packet / "output/samples.json")
                checks["participant_sample_file_matches"] = (
                    saved["frames"] == report["frames"]
                )
        if data["environment"] == "connected":
            import zipzi

            with zipzi.connect(endpoint=Path(data["endpoint"])) as runtime:
                resident = runtime.sessions.attach(data["session_id"])
                checks["connected_original_document_retained"] = (
                    resident.info()["document_id"] == expected["document_id"]
                )
                checks["connected_original_parameters_match_saved"] = (
                    records(resident, v.ObjectKind.PARAMETER) == actual
                )
                decoy = runtime.sessions.attach(data["decoy_session_id"])
                checks["connected_decoy_unchanged"] = (
                    records(decoy, v.ObjectKind.PARAMETER)
                    == expected["decoy_parameters"]
                    and {
                        kind.value: records(decoy, kind)
                        for kind in v.ObjectKind
                        if kind != v.ObjectKind.PARAMETER
                    }
                    == expected["decoy_domains"]
                )
        events = ledger_events(private)
        checks.update(observation_checks(data["task_id"], data, events))
        checks["save_receipt_observed"] = any(
            event.get("path") == "Job.wait"
            and isinstance(_result(event), dict)
            and _result(event).get("kind") == "saved"
            and _result(event).get("published") is True
            and _result(event).get("path") == str(output)
            and isinstance(_result(event).get("durable"), bool)
            and "warnings" in _result(event)
            for event in events
        )
    except Exception as failure:
        error = f"{type(failure).__name__}: {failure}"
    result = {
        "suite": VERSION,
        "task_id": data["task_id"],
        "verified": bool(checks) and all(checks.values()) and error is None,
        "checks": checks,
        "error": error,
    }
    write_json(private / "verification.json", result)
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("setup")
    prepare.add_argument("packet", type=Path)
    prepare.add_argument("private", type=Path)
    prepare.add_argument("task_id", choices=TASKS)
    prepare.add_argument("environment", choices=("local", "connected"))
    prepare.add_argument("--endpoint")
    check = commands.add_parser("verify")
    check.add_argument("packet", type=Path)
    check.add_argument("private", type=Path)
    args = parser.parse_args()
    result = (
        setup(args.packet, args.private, args.task_id, args.environment, args.endpoint)
        if args.command == "setup"
        else verify(args.packet, args.private)
    )
    print(json.dumps(plain(result), indent=2))
    if args.command == "verify" and not result["verified"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
