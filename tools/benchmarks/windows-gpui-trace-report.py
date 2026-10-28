#!/usr/bin/env python3
"""GPUI CPU/atlas and input-to-platform-return diagnosis, not screen latency.

Uses ZIPZI_WINDOW_BENCH traces from the production Editor. Platform calls include
D3D11 rendering/Present on Windows; they are neither GPU timestamps nor DWM scans.
"""
from __future__ import annotations
import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import statistics

PAIRS = {
    'ui_draw_ms': ('ui_draw_begin', 'ui_draw_end'),
    'build_ms': ('ui_build_begin', 'ui_build_end'),
    'prepaint_ms': ('ui_prepaint_begin', 'ui_prepaint_end'),
    'paint_ms': ('ui_paint_begin', 'ui_paint_end'),
    'roots_ms': ('ui_roots_begin', 'ui_roots_end'),
    'platform_draw_cpu_ms': ('platform_draw_begin', 'platform_draw_end'),
}


def distribution(values):
    values = sorted(values)
    if not values:
        return {'n': 0}
    return {'n': len(values), 'p50': statistics.median(values),
            'p95': values[math.ceil(.95 * len(values)) - 1], 'max': values[-1],
            'over_16_67_ms': sum(v > 1000 / 60 for v in values)}


def diagnose(records):
    if not records or records[0].get('type') != 'metadata':
        raise ValueError('missing metadata')
    if records[-1].get('type') != 'end' or records[-1].get('lost_events') != 0:
        raise ValueError('incomplete trace or lost events')
    metadata = records[0]
    events = sorted((r for r in records if r.get('type') == 'event'), key=lambda e: e['at_ns'])
    if not any(e['stage'] == 'benchmark_done' for e in events):
        raise ValueError('benchmark did not finish')
    if any(e['stage'] in ('benchmark_failed', 'render_error') for e in events):
        raise ValueError('benchmark/render error')
    ticks = {e['id']: e for e in events if e['stage'] == 'benchmark_tick'}
    measured = {key: e for key, e in ticks.items() if not e['fields']['warmup']}
    if not measured:
        raise ValueError('no measured inputs')
    parents = {e['id']: e['fields'].get('parent', 0) for e in events if e['stage'] == 'input_begin'}

    def tick_for(input_id):
        seen = set()
        while input_id and input_id not in seen:
            if input_id in ticks:
                return input_id
            seen.add(input_id)
            input_id = parents.get(input_id, 0)
        return 0

    preview_tick = {e['id']: tick_for(e['fields'].get('input', 0))
                    for e in events if e['stage'] == 'scene_begin'}
    published = {e['id']: e['at_ns'] for e in events if e['stage'] == 'published'}
    grouped = defaultdict(list)
    for event in measured.values():
        grouped[event['fields']['mode']].append(event)
    reports = {}
    for mode, phase_ticks in sorted(grouped.items()):
        expected = metadata.get('samples')
        if expected is not None and len(phase_ticks) != expected:
            raise ValueError(f'mode {mode}: {len(phase_ticks)} inputs, expected {expected}')
        start = min(e['at_ns'] for e in phase_ticks)
        last = max(e['at_ns'] for e in phase_ticks)
        # Existing driver settles 500 ms after each gesture; retain the first
        # 250 ms for pending frames, without crossing the next workload warmup.
        end = last + 250_000_000
        later = [e['at_ns'] for e in ticks.values() if e['at_ns'] > last]
        if later:
            end = min(end, min(later))
        selected = [e for e in events if start <= e['at_ns'] <= end]
        upload_frames = {e['fields']['cpu_frame'] for e in selected
                         if e['stage'] == 'image_atlas' and e['fields']['miss']}
        durations = defaultdict(list)
        opens = {}
        for event in selected:
            for label, (begin, finish) in PAIRS.items():
                key = (label, event['id'])
                if event['stage'] == begin:
                    opens[key] = event['at_ns']
                elif event['stage'] == finish and key in opens:
                    duration = (event['at_ns'] - opens.pop(key)) / 1e6
                    durations[label].append(duration)
                    if label == 'ui_draw_ms':
                        control = 'ui_draw_with_upload_ms' if event['id'] in upload_frames else 'ui_draw_without_upload_ms'
                        durations[control].append(duration)
            if event['stage'] == 'image_atlas':
                fields = event['fields']
                durations['atlas_miss_ms' if fields['miss'] else 'atlas_hit_ms'].append(
                    (fields['end_ns'] - event['at_ns']) / 1e6)
            if event['stage'] == 'readback':
                durations['worker_readback_ms'].append(event['fields']['readback_ns'] / 1e6)
        # New image paint -> enclosing CPU frame -> first platform return after
        # that paint. Retained replay may omit paint_image; no join is invented.
        draw_id = 0
        painted = []
        timed_previews = set()
        returns = defaultdict(list)
        draws = {}
        for event in events:
            if event['stage'] == 'ui_draw_begin':
                draw_id = event['id']
                draws[draw_id] = event['at_ns']
            elif event['stage'] == 'ui_draw_end':
                draw_id = 0
            elif event['stage'] in ('compose_image', 'paint_shared_surface') and draw_id:
                tick = preview_tick.get(event['id'], 0)
                if tick in measured and measured[tick]['fields']['mode'] == mode:
                    painted.append((tick, draw_id, event['at_ns']))
                    # Repaints still participate in platform-return joins, but
                    # delivery latency ends at the preview's first paint.
                    if event['id'] in published and event['id'] not in timed_previews:
                        timed_previews.add(event['id'])
                        durations['worker_publish_to_image_paint_ms'].append(
                            (event['at_ns'] - published[event['id']]) / 1e6)
            elif event['stage'] == 'platform_draw_end':
                returns[event['id']].append(event['at_ns'])
        per_tick = defaultdict(list)
        submit_times = defaultdict(list)
        for frame, begin in draws.items():
            if start <= begin <= end and returns[frame]:
                durations['ui_begin_to_platform_return_ms'].append((returns[frame][0] - begin) / 1e6)
        for tick, frame, painted_at in painted:
            submission = next((t for t in returns[frame] if t >= painted_at), None)
            if submission is not None:
                per_tick[tick].append((submission - measured[tick]['at_ns']) / 1e6)
                submit_times[tick].append(submission)
        first_submissions = sorted(min(values) for values in submit_times.values())
        durations['new_image_platform_interval_ms'] = [
            (b - a) / 1e6 for a, b in zip(first_submissions, first_submissions[1:])]
        durations['input_to_first_image_platform_return_ms'] = [min(v) for v in per_tick.values()]
        durations['timer_late_ms'] = [e['fields']['timer_late_ns'] / 1e6 for e in phase_ticks]
        durations['input_interval_ms'] = [(b['at_ns'] - a['at_ns']) / 1e6
                                          for a, b in zip(phase_ticks, phase_ticks[1:])]
        missed = [e for e in selected if e['stage'] == 'image_atlas' and e['fields']['miss']]
        reports[str(mode)] = {
            'inputs': len(phase_ticks), 'inputs_with_image_platform_return': len(per_tick),
            'inputs_without_image_platform_return': len(phase_ticks) - len(per_tick),
            'new_image_platform_returns_per_second': (
                (len(first_submissions) - 1) * 1e9 / (first_submissions[-1] - first_submissions[0])
                if len(first_submissions) > 1 and first_submissions[-1] > first_submissions[0] else None),
            'requests': sum(e['stage'] == 'request' and preview_tick.get(e['id']) in measured for e in selected),
            'atlas_uploaded_bytes': sum(e['fields']['bytes'] for e in missed),
            'image_sizes_bytes': sorted({e['fields']['bytes'] for e in missed}),
            'metrics': {label: distribution(values) for label, values in durations.items()},
        }
    if set(map(str, metadata.get('workloads') or [])) - reports.keys():
        raise ValueError('missing workload')
    return {'metadata': metadata, 'boundary': 'CPU platform draw return; excludes DWM/scanout and OS input delivery',
            'workloads': reports, 'lost_events': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = diagnose([json.loads(line) for line in args.trace.read_text(encoding='utf-8').splitlines()])
    args.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
