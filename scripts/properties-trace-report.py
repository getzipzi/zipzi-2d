#!/usr/bin/env python3
"""Properties native-window report, using the existing frame/texture correlation.

UI-only changes need not request a new model texture. Their UI/GPU work is
reported by measured phase; texture latency is only claimed for correlated requests.
"""
import argparse
import collections
import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location("frame_report", Path(__file__).with_name("frame-trace-report.py"))
frame = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frame)

CASES = """name runtime_id part masks opacity_text opacity_drag multiply_hex
color_blend enabled picker_open_close picker_tabs picker_hex picker_invalid_hex
picker_hue picker_sv picker_rgb_0_text picker_rgb_1_text picker_rgb_2_text
picker_hsv_0_text picker_hsv_1_text picker_hsv_2_text picker_rgb_0_drag
picker_rgb_1_drag picker_rgb_2_drag picker_hsv_0_drag picker_hsv_1_drag
picker_hsv_2_drag picker_done picker_cancel multiply_reset picker_swatches idle picker_idle""".split()


def analyze(records, budget_ms=1000 / 120):
    metadata = next(r for r in records if r.get("type") == "metadata")
    events = sorted((r for r in records if r.get("type") == "event"), key=lambda r: r["at_ns"])
    # Reuse production request/texture-lifetime correlation. UI-only inputs are
    # intentionally not required to create a new canvas texture.
    frame.MODES.update(enumerate(CASES, 210))
    normalized = []
    for record in records:
        record = dict(record)
        if record.get("type") == "metadata":
            record.update(automatic=False, workloads=list(range(210, 210 + len(CASES))))
        elif record.get("stage") == "properties_tick":
            record.update(stage="benchmark_tick", fields={**record["fields"], "mode":210 + record["fields"]["case"]})
        normalized.append(record)
    rows, correlated = frame.analyze(normalized, budget_ms)
    issues = correlated["issues"]
    if not any(e["stage"] == "benchmark_done" for e in events):
        issues.append("properties benchmark did not complete")
    groups = collections.defaultdict(dict)
    for event in events:
        groups[event["id"]][event["stage"]] = event
    handlers_by_parent = collections.defaultdict(list)
    for group in groups.values():
        if "input_begin" in group and "input_end" in group:
            handlers_by_parent[group["input_begin"]["fields"].get("parent")].append(
                (group["input_end"]["at_ns"] - group["input_begin"]["at_ns"])/1e6)
    requested = metadata.get("property_case")
    expected = [CASES.index(requested)] if requested else list(range(len(CASES)))
    result = {"metadata":metadata, "budget_ms":budget_ms, "issues":issues,
              "measurement":"native GPUI dispatch and Metal GPU timestamps; correlated texture presentation where available; excludes OS input delivery",
              "cases":{}}
    for index in expected:
        name = CASES[index]
        ticks = [e for e in events if e["stage"] == "properties_tick" and e["fields"]["case"] == index and not e["fields"]["warmup"]]
        starts = [e for e in events if e["stage"] == "properties_phase" and e["fields"]["case"] == index]
        ends = [e for e in events if e["stage"] == "properties_phase_end" and e["fields"]["case"] == index]
        if len(ticks) != metadata["samples"] or len(starts) != 1 or len(ends) != 1:
            issues.append(f"{name}: incomplete measured workload ({len(ticks)} ticks)")
            continue
        begin, end = ticks[0]["at_ns"], ends[0]["at_ns"]
        selected = [g for g in groups.values() if "ui_draw_begin" in g and begin <= g["ui_draw_begin"]["at_ns"] <= end]
        composed = [g for g in groups.values() if "compose_begin" in g and begin <= g["compose_begin"]["at_ns"] <= end]
        preview = [r for r in rows if r["mode"] == name and not r["warmup"] and r["request_id"] != ""]
        if any(r["outcome"] == "incomplete" for r in preview):
            issues.append(f"{name}: preview requests lack terminal outcomes")
        metrics = {}
        for metric in ("scene_ms", "textures_ms", "sync_ms", "encode_ms", "preview_gpu_ms", "input_to_present_ms"):
            values = [r[metric] for r in preview if metric in r]
            metrics[metric] = {**frame.quantiles(values), "over_budget":sum(v > budget_ms for v in values)}
        for metric, first, last, source in (
            ("ui_draw_ms", "ui_draw_begin", "ui_draw_end", selected),
            ("compose_gpu_ms", "compose_gpu_start", "compose_gpu_end", composed),
            ("drawable_wait_ms", "drawable_wait_begin", "drawable_acquired", composed),
            ("offscreen_gpu_ms", "offscreen_gpu_start", "offscreen_gpu_end", composed),
        ):
            durations = [(g[last]["at_ns"] - g[first]["at_ns"])/1e6 for g in source if first in g and last in g]
            if any(t < 0 for t in durations):
                issues.append(f"{name}: negative {metric}")
            metrics[metric] = frame.quantiles(durations)
            metrics[metric]["over_budget"] = sum(t > budget_ms for t in durations)
        durations = [(groups[t["id"]]["input_end"]["at_ns"] - t["at_ns"])/1e6 for t in ticks if "input_end" in groups[t["id"]]]
        metrics["interaction_wall_ms"] = frame.quantiles(durations)
        # GPUI's nested dispatch scopes exclude the scripted search debounce,
        # popup settle and tooltip waits included in interaction_wall_ms.
        handlers = []
        dispatch_per_tick = []
        slowest_per_tick = []
        for tick in ticks:
            values = handlers_by_parent[tick["id"]]
            handlers.extend(values)
            dispatch_per_tick.append(sum(values))
            slowest_per_tick.append(max(values, default=0))
        metrics["native_event_handler_ms"] = frame.quantiles(handlers)
        metrics["native_event_handlers_per_tick_ms"] = frame.quantiles(dispatch_per_tick)
        metrics["slowest_native_event_per_tick_ms"] = frame.quantiles(slowest_per_tick)
        for stage in ("property_commit", "property_redraw"):
            measured = [e for e in events if e["stage"] == stage and begin <= e["at_ns"] <= end]
            for clock in ("wall", "cpu"):
                values = [e["fields"][f"{clock}_ns"] / 1e6 for e in measured if f"{clock}_ns" in e["fields"]]
                metrics[f"{stage}_{clock}_ms"] = frame.quantiles(values)
        metrics["timer_late_ms"] = frame.quantiles([t["fields"]["timer_late_ns"]/1e6 for t in ticks])
        metrics["actual_input_interval_ms"] = frame.quantiles([(b["at_ns"]-a["at_ns"])/1e6 for a,b in zip(ticks,ticks[1:])])
        checked = [groups[t["id"]].get("properties_checked") for t in ticks]
        if not all(checked):
            issues.append(f"{name}: unchecked input samples")
        changed = sum(e["fields"]["changed"] for e in checked if e)
        idle = name in ("idle", "picker_idle")
        # No new textures are expected for tabs, hover, invalid drafts or static
        # idle. Lack of GPU data during an idle phase is itself a valid result.
        if not idle and not selected:
            issues.append(f"{name}: no measured UI draws")
        if not idle and not any("presented" in g for g in composed):
            issues.append(f"{name}: no native window presentations")
        if any(e["stage"] in ("render_error", "send_error") and begin <= e["at_ns"] <= end for e in events):
            issues.append(f"{name}: render/delivery error")
        result["cases"][name] = {
            "inputs":len(ticks), "changed_samples":changed,
            "phase_seconds":(end-begin)/1e9,
            "ui_draws":len(selected), "window_frames":len(composed),
            "presented_window_frames":sum("presented" in g for g in composed),
            "preview_outcomes":dict(collections.Counter(r["outcome"] for r in preview)),
            "metrics_ms":metrics,
        }
    return rows, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    rows, result = analyze([json.loads(line) for line in args.trace.read_text().splitlines() if line.strip()])
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (args.output / "requests.json").write_text(json.dumps(rows, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(bool(result["issues"]))


if __name__ == "__main__":
    main()
