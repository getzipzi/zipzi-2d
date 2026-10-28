#!/usr/bin/env python3
"""Join async frame boundaries by ID and texture lifetime; keep all request outcomes.

No third-party dependencies. Input begins at GPUI dispatch (or the synthetic SDK
opacity command). presentedTime is a compositor timestamp, not input-to-photon.
"""
import argparse
import bisect
import collections
import csv
import json
import math
from pathlib import Path

MODES = {1: "pan", 2: "zoom", 3: "opacity", 4: "parameter_slider",
         5: "mesh_single_vertex", 6: "mesh_multi_vertex",
         7: "warp_deformer", 8: "rotation_deformer"}
# Each metric is a difference on ONE correlated request, never a sum of percentiles.
PAIRS = {
    "input_handler_ms": ("input_begin", "input_end"),
    "edit_dispatch_ms": ("input_begin", "scene_begin"),
    "scene_ms": ("scene_begin", "scene_end"),
    "mailbox_ms": ("enqueued", "worker_begin"),
    "textures_ms": ("worker_begin", "textures_ready"),
    "sync_ms": ("textures_ready", "model_synced"),
    "encode_ms": ("encode_begin", "preview_commit_begin"),
    "preview_submit_ms": ("worker_begin", "preview_submitted"),
    "preview_queue_ms": ("preview_commit_begin", "preview_gpu_start"),
    "preview_gpu_ms": ("preview_gpu_start", "preview_gpu_end"),
    "preview_wait_return_ms": ("preview_gpu_end", "preview_complete"),
    "delivery_ms": ("published", "ui_received"),
    "ui_wait_ms": ("accepted", "ui_draw_begin"),
    "ui_draw_ms": ("ui_draw_begin", "ui_draw_end"),
    "drawable_wait_ms": ("compose_begin", "drawable_acquired"),
    "compose_encode_ms": ("drawable_acquired", "compose_commit_begin"),
    "compose_queue_ms": ("compose_commit_begin", "compose_gpu_start"),
    "offscreen_gpu_ms": ("offscreen_gpu_start", "offscreen_gpu_end"),
    "offscreen_queue_ms": ("offscreen_commit_begin", "offscreen_gpu_start"),
    "compose_gpu_ms": ("compose_gpu_start", "compose_gpu_end"),
    "compose_scheduled_callback_ms": ("compose_commit_begin", "compose_scheduled"),
    "submit_to_present_ms": ("compose_submitted", "presented"),
    "gpu_end_to_present_ms": ("compose_gpu_end", "presented"),
    "input_to_submit_ms": ("input_begin", "compose_submitted"),
    "input_to_present_ms": ("input_begin", "presented"),
}


def quantiles(values):
    values = sorted(values)
    if not values:
        return {"count": 0}
    return {"count": len(values), **{
        f"p{p}": values[math.ceil(len(values) * p / 100) - 1] for p in (50, 95, 99)
    }, "max": values[-1]}


def merge_intervals(intervals):
    """Union intervals before overlap accounting, including nested windows."""
    merged = []
    for start, end in sorted(intervals):
        if end < start:
            continue
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(end, merged[-1][1]))
        else:
            merged.append((start, end))
    return merged


def overlap_ms(start, end, intervals):
    return sum(max(0, min(end, b) - max(start, a)) for a, b in intervals) / 1e6


def analyze(records, budget_ms=1000 / 120):
    metadata = [r for r in records if r.get("type") == "metadata"]
    ends = [r for r in records if r.get("type") == "end"]
    issues = []
    if len(metadata) != 1 or metadata[0].get("schema") != 1:
        raise ValueError("expected exactly one schema-1 metadata record")
    metadata = metadata[0]
    if len(ends) != 1:
        issues.append("missing or duplicate end record: capture may be truncated")
    if any(r.get("lost_events", 0) for r in ends):
        issues.append("trace writer lost events; timing coverage is incomplete")
    groups = collections.defaultdict(dict)
    events = sorted((r for r in records if r.get("type") == "event"), key=lambda r: r["at_ns"])
    for event in events:
        # compose_surface can occur more than once; joined separately below.
        groups[event["id"]].setdefault(event["stage"], event)
    if metadata.get("automatic") and not any("benchmark_done" in g for g in groups.values()):
        issues.append("automatic benchmark did not complete")
    if any("benchmark_failed" in g for g in groups.values()):
        issues.append("benchmark failed (see application log)")
    if metadata.get("debug_assertions"):
        issues.append("debug build: do not use these numbers for release acceptance")
    for event in events:
        if event["stage"] in ("preview_complete", "compose_complete", "offscreen_complete") and event["fields"].get("status") != 4:
            issues.append(f"GPU failure on {event['stage']} ID {event['id']}")

    drawable_waits = merge_intervals(
        (g.get("drawable_wait_begin", g["compose_begin"])["at_ns"], g["drawable_acquired"]["at_ns"])
        for g in groups.values() if "compose_begin" in g and "drawable_acquired" in g
    )
    compose_intervals = merge_intervals(
        (g[start]["at_ns"], g[end]["at_ns"])
        for g in groups.values()
        for start, end in [("compose_gpu_start", "compose_gpu_end"), ("offscreen_gpu_start", "offscreen_gpu_end")]
        if start in g and end in g
    )

    def ancestor(input_id):
        seen = set()
        while input_id and input_id not in seen:
            seen.add(input_id)
            group = groups[input_id]
            if "benchmark_tick" in group:
                return input_id, group["benchmark_tick"]["fields"]
            input_id = group.get("input_begin", {}).get("fields", {}).get("parent", 0)
        return 0, {}

    # New captures carry identity on the live Metal resource. Address history
    # alone cannot distinguish a preview from an overlay reusing its address.
    # The compose scene owns the immutable texture until encoding is complete.
    publications = collections.defaultdict(list)
    compositions = collections.defaultdict(set)
    for event in events:
        if event["stage"] == "published":
            if "texture" not in event["fields"]:
                # The Windows image bridge publishes CPU pixels, not a Metal
                # texture. Keep its rows, but do not invent a surface identity.
                if metadata.get("platform") != "windows":
                    issues.append(f"published frame {event['id']} has no texture identity")
                continue
            publications[event["fields"]["texture"]].append((event["at_ns"], event["id"]))
        elif event["stage"] == "compose_surface":
            if "preview" in event["fields"]:
                request_id = event["fields"]["preview"]
                if request_id:
                    publication = groups.get(request_id, {}).get("published")
                    if (publication is None or publication["at_ns"] > event["at_ns"]
                            or publication["fields"].get("texture") != event["fields"]["texture"]):
                        issues.append(f"invalid preview surface identity {request_id}")
                    else:
                        compositions[request_id].add(event["id"])
                continue
            history = publications[event["fields"]["texture"]]
            index = bisect.bisect_right(history, (event["at_ns"], math.inf)) - 1
            if index >= 0:
                compositions[history[index][1]].add(event["id"])

    surface_events = [e for e in events if e["stage"] == "compose_surface"]
    tagged_surfaces = sum("preview" in e["fields"] for e in surface_events)
    surface_identity = "preview_id" if tagged_surfaces else "legacy_pointer"
    if tagged_surfaces and tagged_surfaces != len(surface_events):
        issues.append("mixed preview identity and legacy pointer surfaces")
    if not tagged_surfaces:
        legacy_matches = collections.Counter(c for ids in compositions.values() for c in ids)
        if any(count > 1 for count in legacy_matches.values()):
            issues.append("ambiguous legacy texture pointers: non-preview surfaces may reuse preview addresses")

    rows, used_ticks = [], set()
    for request_id, group in list(groups.items()):
        if "request" not in group:
            continue
        input_id = group["scene_begin"]["fields"].get("input", 0)
        tick_id, tick = ancestor(input_id)
        if tick_id:
            used_ticks.add(tick_id)
        row = {"request_id": request_id, "input_id": input_id, "tick_id": tick_id,
               "mode": MODES.get(tick.get("mode"), "manual"), "sample": tick.get("sample", ""),
               "warmup": tick.get("warmup", 0), **group["request"]["fields"]}
        if tick:
            row["timer_late_ms"] = tick["timer_late_ns"] / 1e6
        times = {name: e["at_ns"] for name, e in group.items()}
        for stage in ("input_begin", "input_end"):
            if stage in groups[input_id]:
                times[stage] = groups[input_id][stage]["at_ns"]
        candidates = [groups[c] for c in compositions[request_id]]
        presented = [c for c in candidates if "presented" in c]
        submitted = [c for c in candidates if "compose_submitted" in c]
        if presented:
            compose = min(presented, key=lambda c: c["presented"]["at_ns"])
            row["outcome"] = "presented"
        elif submitted:
            compose = min(submitted, key=lambda c: c["compose_submitted"]["at_ns"])
            row["outcome"] = "submitted_not_presented"
        else:
            compose = None
            row["outcome"] = next((s for s in ("delivery_coalesced", "coalesced", "cancelled", "rejected", "render_error", "send_error") if s in group),
                                  "painted_unconfirmed" if "compose_image" in group else
                                  "accepted_not_composed" if "accepted" in group else "incomplete")
        if compose:
            times.update({name: e["at_ns"] for name, e in compose.items()})
            cpu_id = compose["compose_begin"]["fields"].get("cpu_frame", 0)
            times.update({name: e["at_ns"] for name, e in groups[cpu_id].items()})
            row["composition_id"] = compose["compose_begin"]["id"]
        if "published" in group:
            row.update({k: v for k, v in group["published"]["fields"].items() if k != "texture"})
        for name, (start, end) in PAIRS.items():
            if name == "drawable_wait_ms" and "drawable_wait_begin" in times:
                start = "drawable_wait_begin"
            if start in times and end in times:
                value = (times[end] - times[start]) / 1e6
                if value < 0:
                    issues.append(f"negative {name} for request {request_id}; check clock/correlation")
                row[name] = value
        if "published" in times and "ui_received" in times:
            row["delivery_drawable_overlap_ms"] = overlap_ms(times["published"], times["ui_received"], drawable_waits)
        if "preview_commit_begin" in times and "preview_gpu_start" in times:
            row["preview_queue_compose_overlap_ms"] = overlap_ms(times["preview_commit_begin"], times["preview_gpu_start"], compose_intervals)
        if "presented" in times:
            row["presented_ns"] = times["presented"]
        rows.append(row)
    # No-op, ignored, or dropped-before-request benchmark inputs remain visible.
    for tick_id, group in list(groups.items()):
        if "benchmark_tick" in group and tick_id not in used_ticks:
            tick = group["benchmark_tick"]["fields"]
            rows.append({"request_id": "", "input_id": tick_id, "tick_id": tick_id,
                         "mode": MODES[tick["mode"]], "sample": tick["sample"],
                         "warmup": tick["warmup"], "outcome": "no_request",
                         "timer_late_ms": tick["timer_late_ns"] / 1e6})
    window_frames = collections.Counter(
        "presented" if "presented" in group else "dropped" if "present_dropped" in group else "unconfirmed"
        for group in groups.values() if "compose_begin" in group
    )
    summary = {"metadata": metadata, "budget_ms": budget_ms, "issues": issues,
               "surface_identity": surface_identity,
               "window_frames_including_startup": dict(window_frames), "modes": {}}
    for mode in sorted({r["mode"] for r in rows}):
        selected = [r for r in rows if r["mode"] == mode and not r["warmup"]]
        latencies = [r["input_to_present_ms"] for r in selected if "input_to_present_ms" in r]
        timestamps = sorted({r["presented_ns"] for r in selected if "presented_ns" in r})
        ticks = {r["tick_id"] for r in selected if r["tick_id"]}
        tick_times = sorted(groups[t]["benchmark_tick"]["at_ns"] for t in ticks)
        window_timestamps = sorted({e["at_ns"] for e in events
                                    if e["stage"] == "presented" and tick_times
                                    and tick_times[0] <= e["at_ns"] <= tick_times[-1]})
        def fps(times):
            return (len(times) - 1) * 1e9 / (times[-1] - times[0]) if len(times) > 1 else None
        summary["modes"][mode] = {
            "rows": len(selected), "inputs": len(ticks),
            "window_present_fps": fps(window_timestamps),
            "new_preview_present_fps": fps(timestamps),
            "window_present_interval_ms": quantiles([(b - a) / 1e6 for a, b in zip(window_timestamps, window_timestamps[1:])]),
            "outcomes": dict(collections.Counter(r["outcome"] for r in selected)),
            "metrics_ms": {name: quantiles([r[name] for r in selected if name in r])
                           for name in (*PAIRS, "timer_late_ms", "delivery_drawable_overlap_ms", "preview_queue_compose_overlap_ms")},
            "delivery_drawable_overlap_fraction": (
                sum(r.get("delivery_drawable_overlap_ms", 0) for r in selected) /
                sum(r.get("delivery_ms", 0) for r in selected)
                if sum(r.get("delivery_ms", 0) for r in selected) else None
            ),
            "preview_queue_compose_overlap_fraction": (
                sum(r.get("preview_queue_compose_overlap_ms", 0) for r in selected) /
                sum(r.get("preview_queue_ms", 0) for r in selected)
                if sum(r.get("preview_queue_ms", 0) for r in selected) else None
            ),
            "actual_input_interval_ms": quantiles([(b - a) / 1e6 for a, b in zip(tick_times, tick_times[1:])]),
            "input_to_present_over_budget": sum(t > budget_ms for t in latencies),
            "new_preview_present_interval_ms": quantiles([(b - a) / 1e6 for a, b in zip(timestamps, timestamps[1:])]),
        }
        if metadata.get("automatic") and mode != "manual":
            if len(ticks) != metadata["samples"]:
                issues.append(f"{mode}: expected {metadata['samples']} measured inputs, got {len(ticks)}")
            if not latencies:
                issues.append(f"{mode}: no correlated presentation timestamps")
    workload_ids = metadata.get("workloads")
    if workload_ids is None:  # Historical schema-1 captures predate explicit workloads.
        workload_ids = [4] if metadata.get("parameters_only") else [1, 2, 3]
    if not workload_ids or any(i not in MODES for i in workload_ids):
        issues.append("invalid workload list in metadata")
    expected_modes = {MODES[i] for i in workload_ids if i in MODES}
    if metadata.get("automatic") and not expected_modes.issubset(summary["modes"]):
        issues.append("one or more workloads are missing")
    return rows, summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--budget-ms", type=float, default=1000 / 120)
    args = parser.parse_args()
    records, parse_issues = [], []
    for index, line in enumerate(args.trace.read_text().splitlines(), 1):
        if not line.strip():
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError:
            parse_issues.append(f"invalid/truncated JSON at line {index}")
    rows, summary = analyze(records, args.budget_ms)
    summary["issues"].extend(parse_issues)
    args.output.mkdir(parents=True, exist_ok=True)
    fields = list(dict.fromkeys(key for row in rows for key in row))
    with (args.output / "requests.csv").open("w", newline="") as out:
        writer = csv.DictWriter(out, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    (args.output / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    for mode, data in summary["modes"].items():
        print(f"{mode}: {data['outcomes']}")
        for metric in ("scene_ms", "mailbox_ms", "preview_gpu_ms", "ui_draw_ms", "input_to_submit_ms", "input_to_present_ms"):
            print(f"  {metric}: {data['metrics_ms'][metric]}")
    for issue in summary["issues"]:
        print(f"INCOMPLETE: {issue}")
    return bool(summary["issues"])


if __name__ == "__main__":
    raise SystemExit(main())
