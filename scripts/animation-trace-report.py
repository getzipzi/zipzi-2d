#!/usr/bin/env python3
"""Correlated animation CPU/Metal timings, outcomes and mailbox high watermarks.

GPU end is completed render work; accepted means the UI adopted a texture.
Native compose/present events measure first presentation of each texture.
Headless traces have no presentation events. None measures input-to-photon latency.
"""
import collections
import json
import math
import sys

PAIRS = {
    "cpu_mailbox_ms": ("animation_enqueued", "animation_evaluate_begin"),
    "evaluate_ms": ("animation_evaluate_begin", "animation_evaluate_end"),
    "render_mailbox_ms": ("enqueued", "worker_begin"),
    "render_submit_ms": ("worker_begin", "preview_submitted"),
    "gpu_ms": ("preview_gpu_start", "preview_gpu_end"),
    "enqueue_to_accept_ms": ("animation_enqueued", "accepted"),
}


def summarize(values):
    values = sorted(values)
    return {"n": len(values), **{key: values[min(len(values)-1, math.ceil(q*len(values))-1)]
        for key, q in [("p50", .5), ("p95", .95), ("p99", .99), ("max", 1)]}} if values else {"n": 0}


def report(records):
    events = sorted((r for r in records if r.get("type") == "event"), key=lambda r: r["at_ns"])
    end = [r for r in records if r.get("type") == "end"]
    if not end or end[-1]["lost_events"]:
        raise ValueError("incomplete trace or dropped events")
    if any(e["stage"] == "benchmark_failed" for e in events):
        raise ValueError("native benchmark failed (see application.log)")
    for event in events:
        if event["stage"] in ("preview_complete", "compose_complete", "offscreen_complete") and event["fields"].get("status") != 4:
            raise ValueError("GPU command failed")
    metadata = next((r for r in records if r.get("type") == "metadata"), {})
    groups = collections.defaultdict(dict)
    phases = {}
    textures, compositions = {}, collections.defaultdict(set)
    phase = (-1, 0)
    pending_cpu, pending_render = set(), set()
    max_cpu = max_render = 0
    for event in events:
        key, stage = event["id"], event["stage"]
        groups[key][stage] = event
        if stage == "published":
            textures[event["fields"]["texture"]] = key
        elif stage == "compose_surface" and event["fields"]["texture"] in textures:
            compositions[textures[event["fields"]["texture"]]].add(key)
        if stage == "animation_workload":
            phase = (event["fields"]["model"], event["fields"]["phase"])
        if stage == "animation_enqueued":
            phases[key] = phase
            pending_cpu.add(key)
            max_cpu = max(max_cpu, len(pending_cpu))
        if stage in ("animation_dequeued", "animation_coalesced", "animation_cancelled"):
            pending_cpu.discard(key)
        if stage == "enqueued" and key in phases:
            pending_render.add(key)
            max_render = max(max_render, len(pending_render))
        if stage in ("dequeued", "coalesced", "cancelled"):
            pending_render.discard(key)
    playback = collections.defaultdict(list)
    outcomes = collections.Counter()
    cache = 0
    for key, phase in phases.items():
        g = groups[key]
        outcome = next((s for s in ("accepted", "animation_cancelled", "animation_coalesced", "animation_rejected_cpu", "rejected", "coalesced", "render_error") if s in g), "incomplete")
        outcomes[outcome] += 1
        if "animation_cache" in g:
            fields = g["animation_cache"]["fields"]
            if fields["bytes"] > fields["budget"]:
                raise ValueError("cache exceeded budget")
            cache = max(cache, fields["bytes"])
        presented = [groups[c]["presented"] for c in compositions[key] if "presented" in groups[c]]
        if presented:
            g["presented"] = min(presented, key=lambda e: e["at_ns"])
        if phase[1] == 1:
            playback[phase[0]].append(g)
    result = {"profile": "debug" if metadata.get("debug_assertions") else "release", "lost_events": 0, "cpu_pending_high_water": max_cpu,
              "render_pending_high_water": max_render, "max_checkpoint_bytes": cache,
              "outcomes_all_phases": dict(outcomes), "playback": {}}
    for model, groups in playback.items():
        values = {}
        for name, (start, finish) in PAIRS.items():
            values[name] = summarize([(g[finish]["at_ns"]-g[start]["at_ns"])/1e6
                                     for g in groups if start in g and finish in g])
        accepted = sorted(g["accepted"]["at_ns"] for g in groups if "accepted" in g)
        values["accepted_interval_ms"] = summarize([(b-a)/1e6 for a, b in zip(accepted, accepted[1:])])
        presented = sorted(g["presented"]["at_ns"] for g in groups if "presented" in g)
        values["presented_interval_ms"] = summarize([(b-a)/1e6 for a,b in zip(presented,presented[1:])])
        values["presented"] = len(presented)
        values["presented_hz"] = (len(presented)-1)*1e9/(presented[-1]-presented[0]) if len(presented)>1 else None
        values["requests"] = len(groups)
        values["accepted"] = len(accepted)
        values["accepted_hz"] = (len(accepted)-1)*1e9/(accepted[-1]-accepted[0]) if len(accepted)>1 else 0
        result["playback"][str(model)] = values
    return result


if __name__ == "__main__":
    with open(sys.argv[1]) as source:
        print(json.dumps(report([json.loads(line) for line in source]), indent=2))
