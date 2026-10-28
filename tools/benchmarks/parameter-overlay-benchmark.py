#!/usr/bin/env python3
"""Native parameter-drag overlay diagnosis using one saved Release binary.

Build once with --features test-support,bench-instrumentation before running.
Every process keeps its own config, trace, audits and background-process samples.
"""
from __future__ import annotations
import argparse
import collections
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import time
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('frame_report', Path(__file__).with_name('frame-trace-report.py'))
frame_report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frame_report)
CASES = {'unselected': ('none', 'all', False), 'selected': ('all', 'all', False),
         'no_paint': ('all', 'none', False), 'vertices': ('all', 'vertices', False),
         'edges': ('all', 'edges', False), 'detail': ('all', 'all', True),
         'detail_no_paint': ('all', 'none', True), 'detail_unselected': ('none', 'all', True)}


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def process_snapshot():
    raw = subprocess.check_output(['ps', '-axo', 'pid,ppid,pcpu,comm'], text=True)
    compilers = [line for line in raw.splitlines() if Path(line.split()[-1]).name in
                 ('rustc', 'cargo', 'clang', 'clang++', 'xcodebuild')]
    return {'time': time.time(), 'compilers': compilers, 'processes': raw}


def diagnose(records):
    """Keep setup/warmup outside the measured interval; retain all GPU commands."""
    events = [r for r in records if r.get('type') == 'event']
    ticks = [r for r in events if r['stage'] == 'benchmark_tick' and not r['fields']['warmup']]
    if not ticks:
        raise ValueError('no measured ticks')
    start = min(r['at_ns'] for r in ticks)
    end = max(r['at_ns'] for r in events if r['stage'] == 'parameter_phase_end')
    groups = collections.defaultdict(dict)
    for e in events:
        groups[e['id']][e['stage']] = e
    builds, stages, gpu = collections.defaultdict(list), collections.defaultdict(list), collections.defaultdict(list)
    for g in groups.values():
        if (e := g.get('overlay_build')) and start <= e['at_ns'] < end:
            origin = e['fields']['origin']
            builds[str(origin)].append((e['fields']['end_ns'] - e['at_ns']) / 1e6)
            for name, item in g.items():
                if name.startswith('overlay.'):
                    fields = item['fields']
                    stages[f'{origin}/{name}'].append(fields)
        if (e := g.get('overlay_commit')) and start <= e['at_ns'] < end:
            if any(stage not in g for stage in ('overlay_gpu_start', 'overlay_gpu_end', 'overlay_gpu_complete')):
                raise ValueError('missing overlay GPU timestamps')
            if g['overlay_gpu_complete']['fields']['status'] != 4:
                raise ValueError('overlay GPU failure')
            gpu[str(e['fields']['pass'])].append((g['overlay_gpu_end']['at_ns'] - g['overlay_gpu_start']['at_ns']) / 1e6)
    draws = [g for g in groups.values() if 'ui_draw_begin' in g and 'ui_draw_end' in g
             and start <= g['ui_draw_begin']['at_ns'] < end]
    return {'start_ns': start, 'end_ns': end, 'ticks': len(ticks),
            'ui_draw_ms': frame_report.quantiles([(g['ui_draw_end']['at_ns']-g['ui_draw_begin']['at_ns'])/1e6 for g in draws]),
            'build_ms_by_origin': {k: frame_report.quantiles(v) for k, v in builds.items()},
            'build_sum_ms_by_origin': {k: sum(v) for k, v in builds.items()},
            'stages': {k: {'calls': sum(r['calls'] for r in v),
                           'inclusive_ms': frame_report.quantiles([r['inclusive_ns']/1e6 for r in v]),
                           'self_ms': frame_report.quantiles([r['self_ns']/1e6 for r in v]),
                           'sum_inclusive_ms': sum(r['inclusive_ns'] for r in v)/1e6,
                           'sum_self_ms': sum(r['self_ns'] for r in v)/1e6} for k, v in stages.items()},
            'overlay_gpu_ms_by_pass': {k: frame_report.quantiles(v) for k, v in gpu.items()}}


def analyze(folder, allow_debug=False):
    records = [json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()]
    rows, summary = frame_report.analyze(records)
    unexpected = [issue for issue in summary['issues'] if not (allow_debug and issue == 'debug build: do not use these numbers for release acceptance')]
    if unexpected:
        raise ValueError(unexpected)
    diagnostic = diagnose(records)
    if diagnostic['ticks'] != records[0]['samples']:
        raise ValueError('missing input ticks')
    for name, value in [('summary', summary), ('diagnostic', diagnostic)]:
        (folder/f'{name}.json').write_text(json.dumps(value, indent=2)+'\n')
    return summary, diagnostic


def run(args, folder, case):
    folder.mkdir(parents=True, exist_ok=False)
    selection, paint, detail = CASES[case]
    env = dict(os.environ)
    for key in list(env):
        if key.startswith(('ZIPZI_TRACE_', 'GPUI_TRACE_', 'ZIPZI_BENCH_', 'ZIPZI_DEV_', 'ZIPZI_RUNTIME_')) or key == 'MTL_HUD_ENABLED':
            env.pop(key)
    runtime_dir = tempfile.mkdtemp(prefix='zov-', dir='/tmp')
    (folder/'runtime.json').write_text(json.dumps({'directory': runtime_dir})+'\n')
    env.update(ZIPZI_RUNTIME_DIR=runtime_dir, ZIPZI_CONFIG_DIR=str(folder/'config'), ZIPZI_FRAME_TRACE=str(folder/'trace.jsonl'),
               ZIPZI_WINDOW_BENCH='1', ZIPZI_BENCH_PARAMETERS='1', ZIPZI_BENCH_MODEL=str(args.model),
               ZIPZI_BENCH_SAMPLES=str(args.samples), ZIPZI_BENCH_WARMUP=str(args.warmup),
               ZIPZI_BENCH_HZ=str(args.hz), ZIPZI_BENCH_SELECTION=selection,
               ZIPZI_BENCH_OVERLAY_PAINT=paint, ZIPZI_BENCH_OVERLAY_DETAIL=str(int(detail)),
               ZIPZI_BENCH_UNLAYERED_MARKERS=str(int(args.unlayered_markers)),
               ZIPZI_BENCH_TESSELLATED_EDGES=str(int(args.tessellated_edges)))
    snapshots = [process_snapshot()]
    with (folder/'application.log').open('w') as log:
        process = subprocess.Popen(['caffeinate', '-d', '-i', str(args.binary)], env=env, stdout=log, stderr=log)
        deadline = time.monotonic() + 300
        while process.poll() is None:
            if time.monotonic() > deadline:
                process.terminate()
                raise TimeoutError(folder)
            time.sleep(1)
            snapshots.append(process_snapshot())
    (folder/'background.json').write_text(json.dumps(snapshots, indent=2)+'\n')
    if process.returncode:
        raise RuntimeError(f'{folder}: exit {process.returncode}')
    summary, diagnostic = analyze(folder, args.allow_debug)
    audit = json.loads((folder/'parameter-audit.json').read_text())
    fixture = json.loads((folder/'overlay-audit.json').read_text())
    if audit['changed_meshes'] <= 0 or (selection == 'all' and fixture['vertices'] <= 0):
        raise ValueError('no-op or empty benchmark fixture')
    return {'directory': str(folder), 'case': case,
            'interfered': any(r['compilers'] for r in snapshots),
            'summary': summary, 'diagnostic': diagnostic, 'audit': audit, 'fixture': fixture}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--model', type=Path, default=ROOT/'models/local/mao/runtime/mao_pro.model3.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=360)
    parser.add_argument('--warmup', type=int, default=30)
    parser.add_argument('--hz', type=int, default=120)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--allow-debug', action='store_true', help='diagnosis only; retain the debug-build warning')
    parser.add_argument('--unlayered-markers', action='store_true', help='reference: order each marker individually')
    parser.add_argument('--tessellated-edges', action='store_true', help='reference: use the general cached stroke tessellator')
    parser.add_argument('--cases', nargs='+', choices=CASES, default=['unselected','selected','no_paint','vertices','edges'])
    args = parser.parse_args()
    args.binary, args.model, args.output = args.binary.resolve(), args.model.resolve(), args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    provenance = {'binary': str(args.binary), 'binary_sha256': digest(args.binary),
                  'model': str(args.model), 'model_sha256': digest(args.model),
                  'driver_sha256': digest(Path(__file__)), 'samples': args.samples, 'warmup': args.warmup,
                  'hz': args.hz, 'rounds': args.rounds, 'cases': args.cases,
                  'unlayered_markers': args.unlayered_markers,
                  'tessellated_edges': args.tessellated_edges}
    (args.output/'run.json').write_text(json.dumps(provenance, indent=2)+'\n')
    results = []
    for round in range(1, args.rounds+1):
        cases = args.cases if round % 2 else list(reversed(args.cases))
        for case in cases:
            folder = args.output/f'round-{round}-{case}'
            print(f'RUN {folder.name}', flush=True)
            result = run(args, folder, case)
            results.append(result)
            (args.output/'results.json').write_text(json.dumps(results, indent=2)+'\n')
            metrics = result['summary']['modes']['parameter_slider']
            print(json.dumps({'case': case, 'interfered': result['interfered'],
                              'draw_p95': metrics['metrics_ms']['ui_draw_ms'].get('p95'),
                              'outcomes': metrics['outcomes'], 'audit': result['audit']}), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
