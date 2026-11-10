#!/usr/bin/env python3
"""Offline scheduling study; synthetic timing is NOT native/GPU performance.

Run from the repository root with uv run --locked python. The optional trace
directory reads existing completed benchmark runs; it never launches the Editor.
"""
from __future__ import annotations

import argparse
from bisect import bisect_right
from collections import defaultdict
import json
import math
from pathlib import Path
import statistics


def stats(values):
    values = sorted(values)
    if not values:
        return {"n": 0}
    return {"n": len(values), "min": values[0], "mean": statistics.mean(values),
            "p50": statistics.median(values),
            "p95": values[math.ceil(len(values) * .95) - 1], "max": values[-1]}


def schedule(inputs, policy, frame_ms):
    """Zero-cost ingress/application; frame wins ties. Input indices identify values.

    Policies mirror the two proposed scheduling state machines after gesture
    startup. No OS queue, CPU contention, validation, Release, or GPU is modeled.
    """
    callbacks = [frame_ms * i for i in range(1, math.ceil(inputs[-1] / frame_ms) + 4)]
    events = sorted([(t, 1, i) for i, t in enumerate(inputs)]
                    + [(t, 0, -1) for t in callbacks])
    armed = False
    pending = None
    applied = []
    for now, kind, index in events:
        if kind == 1:
            if policy == "off" or not armed:
                applied.append((now, index))
                armed = policy != "off"
            else:
                pending = index
        elif armed:
            sample, pending = pending, None
            armed = policy == "continuous" and sample is not None
            if sample is not None:
                applied.append((now, sample))
    assert pending is None
    assert applied[-1][1] == len(inputs) - 1, "final value must drain without new input"
    assert all(a[1] < b[1] for a, b in zip(applied, applied[1:]))
    return applied


def synthetic_case(name, inputs, frame_ms=1000 / 60, ready_ms=3., latch_ms=1.):
    start, end = 250., 2000.
    result = {"name": name, "frame_ms": frame_ms,
              "ready_ms": ready_ms, "latch_ms": latch_ms, "policies": {}}
    for policy in ["off", "leading", "continuous"]:
        applied = schedule(inputs, policy, frame_ms)
        selected = [(t, i) for t, i in applied if start <= t < end]
        # Hypothetical independent readiness, no worker queuing. A frame adopts
        # the newest ready result at callback_time + latch_ms, and holds it.
        # This isolates phase effects; it does not predict Windows presentation.
        ready = [t + ready_ms for t, _ in applied]
        presented = []
        for tick in range(math.ceil(end / frame_ms) + 1):
            at = tick * frame_ms + latch_ms
            slot = bisect_right(ready, at) - 1
            if slot >= 0:
                presented.append((at, applied[slot][1]))
        sample_times = [start + i * .25 for i in range(int((end - start) / .25))]
        application_times = [t for t, _ in applied]
        presentation_times = [t for t, _ in presented]
        application_age, presentation_age = [], []
        for now in sample_times:
            index = bisect_right(application_times, now) - 1
            application_age.append(now - inputs[applied[index][1]])
            index = bisect_right(presentation_times, now) - 1
            presentation_age.append(now - inputs[presented[index][1]])
        result["policies"][policy] = {
            "apply_per_second": len(selected) / ((end - start) / 1000),
            "applied_sample_wait_ms": stats([t - inputs[i] for t, i in selected]),
            "wall_sampled_applied_age_ms": stats(application_age),
            "hypothetical_presented_age_ms": stats(presentation_age),
        }
    return result


def simulation():
    frame = 1000 / 60
    cases = []
    for hz in [30, 60, 90, 120, 240, 1000]:
        for phase in [1., 8., 15.]:
            inputs = [phase + i * 1000 / hz for i in range(math.ceil(2000 * hz / 1000))]
            inputs = [t for t in inputs if t < 2000]
            cases.append(synthetic_case(f"{hz}hz_phase{phase:g}", inputs))
    periodic = [1 + i * frame for i in range(120)]
    cases.append(synthetic_case("60hz_after_startup_burst", sorted(periodic + [1.01, 1.02])))
    bursts = [1 + i * frame + j * .01 for i in range(120) for j in range(4)]
    cases.append(synthetic_case("4_per_60hz_burst", bursts))
    for hz in [60, 120, 144]:
        inputs = [1 + i for i in range(1999)]
        cases.append(synthetic_case(f"1000hz_display{hz}", inputs, 1000 / hz))
    cases.append(synthetic_case("240hz_latch4ms", [1 + i * 1000 / 240 for i in range(480)],
                                latch_ms=4.))
    return cases


def read_trace(path):
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    assert records[0]["type"] == "metadata", path
    assert records[-1]["type"] == "end" and records[-1]["lost_events"] == 0, path
    events = [r for r in records if r.get("type") == "event"]
    assert any(e["stage"] == "benchmark_done" for e in events), path
    assert not any(e["stage"] in ("benchmark_failed", "render_error") for e in events), path
    ticks = {e["id"]: e for e in events if e["stage"] == "benchmark_tick"}
    for mode in records[0].get("workloads", []):
        measured = [e for e in ticks.values()
                    if e["fields"]["mode"] == mode and not e["fields"]["warmup"]]
        assert len(measured) == records[0]["samples"], (path, mode, len(measured))
    parents = {e["id"]: e["fields"].get("parent", 0)
               for e in events if e["stage"] == "input_begin"}

    def root(key):
        seen = set()
        while key and key not in ticks and key not in seen:
            seen.add(key)
            key = parents.get(key, 0)
        return key if key in ticks else 0

    immediate = {root(e["id"]) for e in events if e["stage"] == "mesh_move_immediate"}
    scenes = {e["id"]: e for e in events if e["stage"] == "scene_begin"}
    ends = {e["id"]: e for e in events if e["stage"] == "scene_end"}
    requests = {e["id"] for e in events if e["stage"] == "request"}
    groups = defaultdict(lambda: defaultdict(list))
    rows = defaultdict(list)
    for event in events:
        key = root(event["id"])
        tick = ticks.get(key)
        if tick and not tick["fields"]["warmup"]:
            mode = tick["fields"]["mode"]
            if event["stage"] == "input_end" and event["id"] == key:
                groups[mode]["input_sync_ms"].append((event["at_ns"] - tick["at_ns"]) / 1e6)
            if "inclusive_ns" in event["fields"]:
                groups[mode][event["stage"]].append(event["fields"]["inclusive_ns"] / 1e6)
    for key, begin in scenes.items():
        tick_id = root(begin["fields"].get("input", 0))
        tick = ticks.get(tick_id)
        if key not in requests or not tick or tick["fields"]["warmup"]:
            continue
        end = ends[key]
        mode = tick["fields"]["mode"]
        groups[mode]["requested_scene_ms"].append((end["at_ns"] - begin["at_ns"]) / 1e6)
        rows[mode].append((begin["at_ns"], end["at_ns"], tick_id in immediate))
    output = {}
    for mode, measures in groups.items():
        ordered = sorted(rows[mode])
        gaps = [(b[0] - a[1]) / 1e6 for a, b in zip(ordered, ordered[1:])
                if not a[2] and b[2]]
        output[mode] = {"durations": {k: stats(v) for k, v in measures.items()},
                        "tail_to_leading_gap_ms": stats(gaps),
                        "tail_to_leading_under_2ms": sum(v < 2 for v in gaps),
                        "tail_to_leading_under_4ms": sum(v < 4 for v in gaps)}
    run_path = path.with_name("run.json")
    run = json.loads(run_path.read_text(encoding="utf-8")) if run_path.exists() else {}
    return {"path": str(path), "samples": records[0].get("samples"),
            "binary_sha256": run.get("sha256"), "modes": output}


def checks():
    assert schedule([1., 2., 3.], "leading", 10.) == [(1., 0), (10., 2)]
    assert schedule([1., 2., 11.], "leading", 10.) == [(1., 0), (10., 1), (11., 2)]
    assert schedule([1., 2., 11.], "continuous", 10.) == [(1., 0), (10., 1), (20., 2)]
    assert schedule([1., 2., 31.], "continuous", 10.)[-1] == (31., 2)
    for count in range(1, 50):
        for policy in ["off", "leading", "continuous"]:
            schedule([1. + i * .37 for i in range(count)], policy, 10.)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace-dir", type=Path)
    parser.add_argument("--trace", action="append", type=Path, default=[])
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    checks()
    if args.trace_dir and not args.trace_dir.is_dir():
        parser.error(f"trace directory does not exist: {args.trace_dir}")
    paths = args.trace + (sorted(args.trace_dir.glob("*/trace.jsonl")) if args.trace_dir else [])
    result = {"assumptions": "Zero-cost dispatch; fixed frames; independent 3ms readiness; "
              "1ms latch unless named otherwise; 250..2000ms observation; no OS/CPU/GPU model.",
              "synthetic": simulation(), "existing_traces": [read_trace(p) for p in paths]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(result['synthetic'])} synthetic cases and {len(paths)} existing traces to {args.output}")


if __name__ == "__main__":
    main()
