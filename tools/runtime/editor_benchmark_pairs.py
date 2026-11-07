#!/usr/bin/env python3
"""Opt-in paired Editor timing runner; see tools/benchmarks/README.md."""
import hashlib
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import time
import tempfile
from contextlib import contextmanager
ROOT = pathlib.Path(__file__).resolve().parents[2]
import argparse

PARAMETER_MODES = {
    'parameter-track': ('track', 'none'),
    'parameter-number': ('number', 'none'),
    'parameter-pad': ('pad', 'none'),
    'parameter-selected': ('track', 'all'),
}


@contextmanager
def isolated_runtime(environment):
    """Each measured process owns a private, short Unix endpoint.

    A resident desktop host must never receive a benchmark's reopen request.
    Keep the endpoint out of long artifact paths and discard inherited owner fds.
    """
    with tempfile.TemporaryDirectory(prefix='zz-r2-', dir='/tmp') as directory:
        isolated = dict(environment)
        isolated.pop('ZIPZI_RUNTIME_OWNER_FD', None)
        isolated.update(ZIPZI_RUNTIME_DIR=directory, ZIPZI_RUNTIME_PROFILE='benchmark',
                        ZIPZI_CACHE_DIR=str(pathlib.Path(directory) / 'cache'), TMPDIR=directory)
        yield isolated


def pair_order(index):
    """Counterbalance startup/load drift instead of always measuring baseline first."""
    return ('baseline', 'candidate') if index % 2 == 0 else ('candidate', 'baseline')


def main():
    parser = argparse.ArgumentParser(description='Run five alternating Release Editor benchmark pairs from prebuilt, copied executables; no compilation during timing.')
    parser.add_argument('--binaries', type=pathlib.Path, required=True, help='directory containing baseline/candidate-{property-chain,window,timeline,interaction}')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--model', type=pathlib.Path, required=True)
    parser.add_argument('--pairs', type=int, default=5, help='at least five process pairs; all pairs are retained')
    parser.add_argument('--mesh', help='explicit editable Mesh UUID for single- and multi-vertex drags')
    parser.add_argument('--warp', help='explicit editable Warp UUID from the model fixture audit')
    parser.add_argument('--parameter-id', help='require this parameter UUID in the native drag audit')
    parser.add_argument('--physics-case', help='select one Physics inspector workload')
    parser.add_argument('--resume', action='store_true', help='reuse completed runs only when the executable hash matches')
    parser.add_argument('--read-batches', action='store_true', help='Include continuous CPU/Preview/GPU batches in the native interaction case')
    parser.add_argument('modes', nargs='+', choices=[
        'first-frame', 'cpu', 'properties', 'color', 'physics', 'navigation',
        *PARAMETER_MODES, 'deformations', 'animation', 'timeline', 'interaction',
    ])
    args = parser.parse_args()
    if args.pairs < 5:
        parser.error('at least five pairs are required')
    OUT = args.output.resolve()
    OUT.mkdir(parents=True, exist_ok=True)
    BIN = args.binaries.resolve()

    def module(name, file):
        spec = importlib.util.spec_from_file_location(name, ROOT / file)
        mod = importlib.util.module_from_spec(spec)
        sys.modules[name] = mod
        spec.loader.exec_module(mod)
        return mod
    chain = module('chain', 'tools/benchmarks/property-chain-benchmark.py')
    frame = module('frame_trace_report', 'tools/benchmarks/frame-trace-report.py')
    first_frame = module('first_frame', 'tools/benchmarks/first-frame-trace-report.py')
    props = module('properties', 'tools/benchmarks/properties-trace-report.py')
    anim = module('animation', 'tools/benchmarks/animation-trace-report.py')
    model = args.model.resolve()
    env0 = dict(os.environ)
    for key in list(env0):
        if key.startswith(('ZIPZI_BENCH', 'ZIPZI_WINDOW_BENCH', 'ZIPZI_FRAME_TRACE', 'ZIPZI_TIMELINE', 'ZIPZI_RUNTIME_INTERACTION', 'ZIPZI_DEV')) or key == 'MTL_HUD_ENABLED':
            env0.pop(key)
    for mode in args.modes:
        for pair in range(args.pairs):
            for side in pair_order(pair):
                directory = OUT / f'{mode}-{pair + 1}-{side}'
                directory.mkdir(exist_ok=True)
                env = dict(env0)
                if mode == 'cpu':
                    cmd = [str(BIN / f'{side}-property-chain'), str(model), '60', '10', '12', 'all', 'retained']
                elif mode in ['timeline', 'interaction']:
                    env.update(ZIPZI_TIMELINE_MODEL=str(model), ZIPZI_TIMELINE_REQUIRE_ASSET='1', ZIPZI_TIMELINE_SAMPLES='60', ZIPZI_TIMELINE_OUTPUT=str(directory), ZIPZI_RUNTIME_INTERACTION_OUTPUT=str(directory), ZIPZI_RUNTIME_INTERACTION_SAMPLES='60')
                    cmd = [str(BIN / f'{side}-{mode}')]
                    if mode == 'interaction' and args.read_batches:
                        env['ZIPZI_RUNTIME_INTERACTION_READS'] = '1'
                else:
                    env.update(ZIPZI_CONFIG_DIR=str(directory / 'config'), ZIPZI_FRAME_TRACE=str(directory / 'trace.jsonl'), ZIPZI_WINDOW_BENCH='1', ZIPZI_BENCH_MODEL=str(model), ZIPZI_BENCH_HZ='120', ZIPZI_BENCH_SAMPLES='120', ZIPZI_BENCH_WARMUP='10')
                    if mode == 'first-frame':
                        env.update(ZIPZI_BENCH_FIRST_FRAME='1', ZIPZI_BENCH_SAMPLES='1', ZIPZI_BENCH_WARMUP='0')
                    if mode == 'properties':
                        env.update(ZIPZI_BENCH_PROPERTIES='1', ZIPZI_BENCH_PROPERTY_CASE='opacity_drag', ZIPZI_BENCH_SAMPLES='30')
                    if mode == 'color':
                        env.update(ZIPZI_BENCH_PROPERTIES='1', ZIPZI_BENCH_PROPERTY_CASE='picker_rgb_0_drag', ZIPZI_BENCH_SAMPLES='30')
                    if mode == 'physics':
                        env.update(ZIPZI_BENCH_PROPERTIES='1', ZIPZI_BENCH_PROPERTY_SUITE='physics', ZIPZI_BENCH_SAMPLES='40', ZIPZI_BENCH_WARMUP='5')
                        if args.physics_case:
                            env['ZIPZI_BENCH_PROPERTY_CASE'] = args.physics_case
                    if mode == 'navigation':
                        env['ZIPZI_BENCH_NAVIGATION_ONLY'] = '1'
                    if mode in PARAMETER_MODES:
                        control, selection = PARAMETER_MODES[mode]
                        env.update(ZIPZI_BENCH_PARAMETERS='1', ZIPZI_BENCH_PARAMETER_INPUT=control, ZIPZI_BENCH_SELECTION=selection)
                    if mode == 'deformations':
                        env['ZIPZI_BENCH_DEFORMATIONS'] = '1'
                        if args.mesh:
                            env['ZIPZI_BENCH_MESH'] = args.mesh
                        if args.warp:
                            env['ZIPZI_BENCH_WARP'] = args.warp
                    if mode == 'animation':
                        env.update(ZIPZI_BENCH_ANIMATION='1', ZIPZI_BENCH_SAMPLES='600')
                    cmd = ['/usr/bin/caffeinate', '-d', '-i', str(BIN / f'{side}-window')]
                binary = pathlib.Path(cmd[-1] if mode not in ['cpu', 'timeline', 'interaction'] else cmd[0])
                fingerprint = hashlib.sha256(binary.read_bytes()).hexdigest()
                identity = {'binary_sha256': fingerprint, 'model': str(model), 'model_sha256': hashlib.sha256(model.read_bytes()).hexdigest()}
                identity['pair_order'] = list(pair_order(pair))
                identity['workload'] = {name: env.get(name) for name in ['ZIPZI_BENCH_FIRST_FRAME', 'ZIPZI_BENCH_PROPERTY_CASE', 'ZIPZI_BENCH_WARP', 'ZIPZI_BENCH_SAMPLES', 'ZIPZI_BENCH_WARMUP', 'ZIPZI_RUNTIME_INTERACTION_READS', 'ZIPZI_RUNTIME_INTERACTION_SAMPLES']}
                if args.mesh and mode == 'deformations':
                    identity['workload']['ZIPZI_BENCH_MESH'] = args.mesh
                if args.parameter_id and mode in PARAMETER_MODES:
                    identity['workload']['parameter_id'] = args.parameter_id
                for name in ('ZIPZI_BENCH_NAVIGATION_ONLY', 'ZIPZI_BENCH_PARAMETER_INPUT',
                             'ZIPZI_BENCH_SELECTION', 'ZIPZI_BENCH_PROPERTY_SUITE'):
                    if name in env:
                        identity['workload'][name] = env[name]
                if (directory / 'summary.json').exists():
                    if not args.resume or json.loads((directory / 'identity.json').read_text()) != identity:
                        raise ValueError(f'refusing to reuse incompatible run: {directory}')
                    continue
                if (directory / 'trace.jsonl').exists() or (directory / 'samples.jsonl').exists():
                    raise ValueError(f'incomplete run exists: {directory}; preserve it and use a fresh output directory')
                (directory / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
                started = time.time()
                print('start', mode, pair + 1, side, flush=True)
                with isolated_runtime(env) as environment, (directory / 'run.log').open('w') as log, (directory / 'samples.jsonl').open('w') as samples:
                    subprocess.run(['/usr/bin/time', '-l', *cmd], cwd=ROOT, env=environment, stdout=samples if mode == 'cpu' else log, stderr=log, check=True, timeout=600)
                if mode == 'cpu':
                    summary = chain.summarize([json.loads(l) for l in (directory / 'samples.jsonl').read_text().splitlines()])
                elif mode in ['timeline', 'interaction']:
                    summary = json.loads((directory / 'results.json').read_text())
                    assert summary['profile'] == 'release'
                else:
                    records = [json.loads(l) for l in (directory / 'trace.jsonl').read_text().splitlines()]
                    if mode == 'first-frame':
                        summary = first_frame.analyze(records)
                    elif mode in ['properties', 'color', 'physics']:
                        _, summary = props.analyze(records)
                    elif mode == 'animation':
                        summary = anim.report(records)
                    else:
                        _, summary = frame.analyze(records)
                    if summary.get('issues'):
                        raise ValueError(summary['issues'])
                    if mode in PARAMETER_MODES:
                        audit = json.loads((directory / 'parameter-audit.json').read_text())
                        overlay = json.loads((directory / 'overlay-audit.json').read_text())
                        control, selection = PARAMETER_MODES[mode]
                        if args.parameter_id and audit['parameter_id'] != args.parameter_id:
                            raise ValueError(f"unexpected parameter: {audit['parameter_name']} ({audit['parameter_id']})")
                        if audit['changed_meshes'] <= 0 or audit['changed_parameters'] < (2 if control == 'pad' else 1):
                            raise ValueError('parameter input did not change the expected pose')
                        if (overlay['options']['input'], overlay['options']['selection']) != (control, selection):
                            raise ValueError('parameter input or selection override did not take effect')
                        if (audit['selection_count'] != len(overlay['selection'])
                                or bool(overlay['selection']) != (selection == 'all')
                                or (selection == 'all' and overlay['vertices'] <= 0)):
                            raise ValueError('parameter workload used a different overlay selection')
                (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
                print('done', mode, pair + 1, side, round(time.time() - started, 1), 'seconds', flush=True)


if __name__ == "__main__":
    main()
