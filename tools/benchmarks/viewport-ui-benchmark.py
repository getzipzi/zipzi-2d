#!/usr/bin/env python3
"""Compare viewport rendering backends or deformer display in real windows.

Build once with test-support,bench-instrumentation. Each pair uses the same
copied Release binary, private Runtime/config and counterbalanced process order.
Input starts at GPUI dispatch; presentation is a compositor timestamp.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('parameter_overlay', Path(__file__).with_name('parameter-overlay-benchmark.py'))
overlay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(overlay)
frame = overlay.frame_report
SCENES = {'navigation': [1, 2, 3], 'parameter_unselected': [4],
          'parameter_selected': [4], 'deformations': [5, 6, 7, 8]}


def pair_order(index, comparison='renderers'):
    pair = ('on', 'off') if comparison == 'deformers' else ('native', 'gpui')
    return pair if index % 2 == 0 else tuple(reversed(pair))


def environment(base, args, folder, runtime, scene, renderer):
    env = {k: v for k, v in base.items()
           if not k.startswith(('ZIPZI_TRACE_', 'GPUI_TRACE_', 'ZIPZI_BENCH_',
                                'ZIPZI_DEV_', 'ZIPZI_RUNTIME_', 'ZIPZI_CONFIG_',
                                'ZIPZI_FRAME_TRACE', 'ZIPZI_WINDOW_BENCH'))
           and k != 'MTL_HUD_ENABLED'}
    env.update(ZIPZI_RUNTIME_DIR=runtime, ZIPZI_RUNTIME_PROFILE='benchmark',
               ZIPZI_CONFIG_DIR=str(folder/'config'), ZIPZI_FRAME_TRACE=str(folder/'trace.jsonl'),
               ZIPZI_WINDOW_BENCH='1', ZIPZI_BENCH_MODEL=str(args.model),
               ZIPZI_BENCH_SAMPLES=str(args.samples), ZIPZI_BENCH_WARMUP=str(args.warmup),
               ZIPZI_BENCH_HZ=str(args.hz))
    if renderer in ('on', 'off'):
        env['ZIPZI_BENCH_REGULAR_DEFORMERS'] = renderer
        if scene == 'navigation':
            env['ZIPZI_BENCH_NAVIGATION_ONLY'] = '1'
    elif renderer == 'gpui':
        # Historical trace override: keep batched lines, disable native geometry.
        env['ZIPZI_TRACE_BATCH_MESH_EDGES'] = '1'
    if scene.startswith('parameter_'):
        env.update(ZIPZI_BENCH_PARAMETERS='1',
                   ZIPZI_BENCH_SELECTION='all' if scene == 'parameter_selected' else 'none',
                   ZIPZI_BENCH_OVERLAY_DETAIL=str(int(args.detail)))
    elif scene == 'deformations':
        env.update(ZIPZI_BENCH_DEFORMATIONS='1', ZIPZI_BENCH_WARP=args.warp)
    return env


def validate(records, summary, scene, renderer, samples):
    if summary['issues']:
        raise ValueError(summary['issues'])
    metadata = summary['metadata']
    native = renderer != 'gpui'
    if metadata['diagnostics']['native_canvas_overlay'] != native:
        raise ValueError('renderer override was not applied')
    deformer_comparison = renderer in ('on', 'off')
    if deformer_comparison and metadata.get('regular_deformers') != (renderer == 'on'):
        raise ValueError('deformer display override was not applied')
    workloads = [1, 2] if deformer_comparison and scene == 'navigation' else SCENES[scene]
    expected = {frame.MODES[i] for i in workloads}
    # Initial import/selection requests have no benchmark tick and are retained
    # by the shared reporter as "manual". They are outside every measured mode.
    measured = {name: mode for name, mode in summary['modes'].items() if name != 'manual'}
    if set(measured) != expected:
        raise ValueError('wrong viewport workloads')
    if any(m['inputs'] != samples for m in measured.values()):
        raise ValueError('missing measured inputs')
    draws = [r for r in records if r.get('stage') == 'canvas_overlay_draw']
    if native != bool(draws):
        raise ValueError('native canvas path did not match the requested renderer')
    if any(r['fields'].get('status') != 4 for r in records
           if r.get('stage') == 'overlay_gpu_complete'):
        raise ValueError('native overlay GPU failure')


def run(args, folder, scene, renderer):
    folder.mkdir()
    snapshots = [overlay.process_snapshot()]
    with tempfile.TemporaryDirectory(prefix='zvp-', dir='/tmp') as runtime:
        env = environment(os.environ, args, folder, runtime, scene, renderer)
        (folder/'environment.json').write_text(json.dumps(
            {k: v for k, v in env.items() if k.startswith(('ZIPZI_', 'GPUI_'))}, indent=2)+'\n')
        with (folder/'application.log').open('w') as log:
            process = subprocess.Popen(['caffeinate', '-d', '-i', str(args.binary)],
                                       cwd=ROOT, env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 300
            try:
                while process.poll() is None:
                    if time.monotonic() > deadline:
                        raise TimeoutError(str(folder))
                    time.sleep(1)
                    snapshots.append(overlay.process_snapshot())
            finally:
                if process.poll() is None:
                    # caffeinate launches the Editor as its child. Terminate both
                    # so a timed-out benchmark cannot steal later runs' focus.
                    subprocess.run(['pkill', '-TERM', '-P', str(process.pid)], check=False)
                    process.terminate()
                    process.wait(timeout=10)
        if process.returncode:
            raise RuntimeError(f'{folder}: exit {process.returncode}')
    (folder/'background.json').write_text(json.dumps(snapshots, indent=2)+'\n')
    records = [json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()]
    _, summary = frame.analyze(records, budget_ms=1000/args.hz)
    validate(records, summary, scene, renderer, args.samples)
    (folder/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    fixtures = [json.loads(line.split('benchmark fixture: ', 1)[1])
                for line in (folder/'application.log').read_text().splitlines()
                if 'benchmark fixture: ' in line]
    result = {'directory': str(folder), 'scene': scene,
              'renderer': 'native' if renderer in ('on', 'off') else renderer, 'variant': renderer,
              'interfered': any(s['compilers'] for s in snapshots),
              'summary': summary, 'fixtures': fixtures}
    if renderer in ('on', 'off'):
        audit = json.loads((folder/'deformer-audit.json').read_text())
        outlines = audit['unselected_warp_outlines'] + audit['unselected_rotation_bones']
        if audit['visible'] != (renderer == 'on') or ((outlines > 0) != (renderer == 'on')):
            raise ValueError('deformer display fixture did not match the requested visibility')
        result['deformer-audit'] = audit
    if scene.startswith('parameter_'):
        for name in ('parameter-audit', 'overlay-audit'):
            result[name] = json.loads((folder/f'{name}.json').read_text())
        if result['parameter-audit']['changed_meshes'] <= 0:
            raise ValueError('parameter workload did not deform the model')
        if scene == 'parameter_selected' and result['overlay-audit']['vertices'] <= 0:
            raise ValueError('empty selected overlay')
        if args.detail:
            result['diagnostic'] = overlay.diagnose(records)
            (folder/'diagnostic.json').write_text(json.dumps(result['diagnostic'], indent=2)+'\n')
    return result


def check_pair(pair):
    if pair[0]['fixtures'] != pair[1]['fixtures']:
        raise ValueError('deformation target or component IDs differ within a pair')
    for name in ('vertices', 'edges', 'boundary_edges', 'selection', 'model_meshes', 'model_parameters'):
        if pair[0].get('overlay-audit', {}).get(name) != pair[1].get('overlay-audit', {}).get(name):
            raise ValueError(f'parameter overlay fixture differs: {name}')
    if pair[0].get('parameter-audit', {}).get('parameter_id') != pair[1].get('parameter-audit', {}).get('parameter_id'):
        raise ValueError('parameter identity differs within a pair')
    for name in ('mesh_vertices', 'mesh_edges', 'mesh_boundary_edges', 'selected_objects', 'history'):
        if pair[0].get('deformer-audit', {}).get(name) != pair[1].get('deformer-audit', {}).get(name):
            raise ValueError(f'deformer display comparison changed Mesh selection or history: {name}')


def await_compiler_quiet():
    deadline = time.monotonic() + 600
    quiet = 0
    while quiet < 3:
        snapshot = overlay.process_snapshot()
        quiet = 0 if snapshot['compilers'] else quiet+1
        if snapshot['compilers']:
            print('WAIT background compiler activity', flush=True)
        if time.monotonic() > deadline:
            raise TimeoutError('background compilers did not become idle')
        time.sleep(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=4)
    parser.add_argument('--samples', type=int, default=360)
    parser.add_argument('--warmup', type=int, default=30)
    parser.add_argument('--hz', type=int, default=120)
    parser.add_argument('--max-attempts', type=int, default=4, help='retry an entire compiler-interfered pair; retain rejected runs')
    parser.add_argument('--comparison', choices=('renderers', 'deformers'), default='renderers',
                        help='deformers: native rendering with the regular deformer display on/off')
    parser.add_argument('--warp', help='explicit audited editable Warp UUID for the supplied model')
    parser.add_argument('--scenes', nargs='+', choices=SCENES, default=list(SCENES))
    parser.add_argument('--detail', action='store_true', help='separate diagnostic run; parameter scenes only')
    args = parser.parse_args()
    if args.rounds < 1 or args.samples < 1 or args.warmup < 0 or args.max_attempts < 1 or not 1 <= args.hz <= 1000:
        parser.error('invalid sampling configuration')
    if 'deformations' in args.scenes and not args.warp:
        parser.error('--warp is required for the deformations fixture')
    if args.detail and any(not scene.startswith('parameter_') for scene in args.scenes):
        parser.error('--detail requires parameter scenes')
    if args.comparison == 'deformers' and any(scene not in ('navigation', 'parameter_unselected', 'parameter_selected') for scene in args.scenes):
        parser.error('deformer display comparison supports navigation and parameter scenes')
    args.model, args.binary, args.output = args.model.resolve(), args.binary.resolve(), args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    source_binary = args.binary
    args.binary = args.output/'zipzi-editor'
    shutil.copy2(source_binary, args.binary)
    provenance = {**vars(args), 'source_binary': str(source_binary),
                  'binary_sha256': overlay.digest(args.binary), 'driver_sha256': overlay.digest(Path(__file__)),
                  'model_files': {str(p.relative_to(args.model.parent)): overlay.digest(p)
                                  for p in sorted(args.model.parent.rglob('*')) if p.is_file()}}
    (args.output/'run.json').write_text(json.dumps(provenance, default=str, indent=2)+'\n')
    (args.output/'source.diff').write_bytes(subprocess.check_output(['git', 'diff', 'HEAD'], cwd=ROOT))
    (args.output/'source-head.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT))
    # New source files are absent from git diff until staged. Preserve them too.
    untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard',
                                         '-z', '--', 'crates', 'tools'], cwd=ROOT)
    for name in filter(None, untracked.decode().split('\0')):
        destination = args.output/'untracked-source'/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT/name, destination)
    with (args.output/'machine.txt').open('w') as machine:
        for command in (['sw_vers'], ['sysctl', 'hw.model', 'machdep.cpu.brand_string', 'hw.memsize'],
                        ['system_profiler', 'SPDisplaysDataType']):
            subprocess.run(command, stdout=machine, stderr=machine, check=True)
    results, attempts = [], []
    for index in range(args.rounds):
        for scene in args.scenes if index % 2 == 0 else reversed(args.scenes):
            for attempt in range(1, args.max_attempts+1):
                await_compiler_quiet()
                pair = []
                for renderer in pair_order(index, args.comparison):
                    folder = args.output/f'round-{index+1}-{scene}-attempt-{attempt}-{renderer}'
                    print(f'RUN {folder.name}', flush=True)
                    result = run(args, folder, scene, renderer)
                    result.update(round=index+1, attempt=attempt)
                    attempts.append(result)
                    pair.append(result)
                    (args.output/'attempts.json').write_text(json.dumps(attempts, indent=2)+'\n')
                    print(json.dumps({'renderer': renderer, 'interfered': result['interfered'],
                                      'ui_draw_p95': {k: v['metrics_ms']['ui_draw_ms'].get('p95')
                                                      for k, v in result['summary']['modes'].items() if k != 'manual'}}), flush=True)
                check_pair(pair)
                if not any(r['interfered'] for r in pair):
                    results.extend(pair)
                    (args.output/'results.json').write_text(json.dumps(results, indent=2)+'\n')
                    break
                print('REJECT compiler-interfered pair; retain both runs and retry', flush=True)
            else:
                raise ValueError('background compilers interfered with every pair attempt')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
