#!/usr/bin/env python3
"""Compare per-process P50/P95 medians and ranges under the frozen R2 gates."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import statistics


def normalized(mode: str, report: dict) -> dict:
    result = {}

    def add(name, data, *, cpu=True, scale=1):
        p50 = data.get('p50', data.get('p50_ms', data.get('median_us')))
        p95 = data.get('p95', data.get('p95_ms', data.get('p95_us')))
        if p50 is None or p95 is None:
            raise ValueError(f'{mode}/{name}: missing latency samples')
        result[f'{mode}/{name}'] = dict(p50=p50*scale, p95=p95*scale, kind='cpu' if cpu else 'display')

    if mode == 'cpu':
        for name, data in report['measurements'].items():
            add(name, data, scale=0.001)
    elif mode in ('properties', 'color', 'deformations'):
        cases = report['cases'] if mode in ('properties', 'color') else report['modes']
        for case, data in cases.items():
            if case in ('manual', 'idle', 'picker_idle', 'physics_idle'):
                continue  # no measured input latency; retained in raw reports/work-count gates
            metrics = data['metrics_ms']
            for name in ('input_handler_ms', 'native_event_handlers_per_tick_ms', 'ui_draw_ms', 'scene_ms', 'input_to_present_ms'):
                if name in metrics and metrics[name].get('count', 0):
                    add(f'{case}/{name}', metrics[name], cpu=name != 'input_to_present_ms')
    elif mode == 'animation':
        for case, metrics in report['playback'].items():
            for name in ('evaluate_cpu_ms', 'render_cpu_ms', 'ui_draw_ms', 'enqueue_to_accept_ms', 'presented_interval_ms'):
                add(f'{case}/{name}', metrics[name], cpu=name.endswith('cpu_ms') or name == 'ui_draw_ms')
    elif mode == 'interaction':
        for case, metrics in report['measurements'].items():
            for name, data in metrics.items():
                add(f'{case}/{name}', data, cpu=name == 'dispatch')
    elif mode == 'timeline':
        for case in ('real_motion_performance', 'performance'):
            for name in ('scroll_dispatch_draw', 'drag_dispatch', 'drag_dispatch_to_current_metal_draw', 'seek_to_current_metal_draw'):
                add(f'{case}/{name}', report[case][name], cpu=name == 'drag_dispatch')
        # Canvas navigation/dock resize add their own checked per-input samples.
        def visit(value, prefix):
            if isinstance(value, dict):
                if 'p50_ms' in value:
                    add(prefix, value, cpu='dispatch' in prefix and 'draw' not in prefix)
                else:
                    for name, nested in value.items():
                        visit(nested, f'{prefix}/{name}')
        for case in ('canvas_navigation', 'dock_resize'):
            visit(report[case], case)
    else:
        raise ValueError(mode)
    if not result:
        raise ValueError(f'{mode}: no measured cases')
    return result


def compare(baseline: list[dict], candidate: list[dict]) -> dict:
    if len(baseline) < 5 or len(baseline) != len(candidate):
        raise ValueError('at least five matched process pairs are required')
    keys = set(baseline[0])
    if any(set(run) != keys for run in baseline+candidate):
        raise ValueError('inconsistent or missing measured cases')
    rows = {}
    for key in sorted(keys):
        kind = baseline[0][key]['kind']
        if any(run[key]['kind'] != kind for run in baseline+candidate):
            raise ValueError(f'{key}: inconsistent timing boundary')
        row = dict(kind=kind)
        for percentile, ratio in [('p50', 0.10), ('p95', 0.15)]:
            def group(runs):
                values = [run[key][percentile] for run in runs]
                return dict(processes_ms=values, median_ms=statistics.median(values), range_ms=[min(values), max(values)])
            before, after = group(baseline), group(candidate)
            margin = max(before['median_ms']*ratio, 0.2 if kind == 'cpu' else 1.0)
            delta = after['median_ms'] - before['median_ms']
            row[percentile] = dict(baseline=before, candidate=after, delta_ms=delta, allowed_increase_ms=margin, passed=delta <= margin)
        row['passed'] = all(row[p]['passed'] for p in ('p50', 'p95'))
        rows[key] = row
    return dict(pairs=len(baseline), passed=all(row['passed'] for row in rows.values()), measurements=rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=5)
    parser.add_argument('--modes', nargs='+', default=['cpu','properties','color','deformations','animation','timeline','interaction'])
    args = parser.parse_args()
    baseline, candidate = [], []
    for index in range(1, args.pairs + 1):
        for side, runs in [('baseline', baseline), ('candidate', candidate)]:
            merged = {}
            for mode in args.modes:
                report = json.loads((args.directory/f'{mode}-{index}-{side}'/'summary.json').read_text())
                merged.update(normalized(mode, report))
            runs.append(merged)
    report = compare(baseline, candidate)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    failures = [name for name, row in report['measurements'].items() if not row['passed']]
    print(json.dumps(dict(passed=report['passed'], measurements=len(report['measurements']), failures=failures)))
    return not report['passed']


if __name__ == '__main__':
    raise SystemExit(main())
