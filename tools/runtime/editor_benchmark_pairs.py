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
ROOT = pathlib.Path(__file__).resolve().parents[2]
import argparse
def main():
    parser = argparse.ArgumentParser(description='Run five alternating Release Editor benchmark pairs from prebuilt, copied executables; no compilation during timing.')
    parser.add_argument('--binaries', type=pathlib.Path, required=True, help='directory containing baseline/candidate-{property-chain,window,timeline,interaction}')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--model', type=pathlib.Path, required=True)
    parser.add_argument('--pairs', type=int, default=5, help='at least five process pairs; all pairs are retained')
    parser.add_argument('--warp', help='explicit editable Warp UUID from the model fixture audit')
    parser.add_argument('--resume', action='store_true', help='reuse completed runs only when the executable hash matches')
    parser.add_argument('modes', nargs='+', choices=['cpu', 'properties', 'color', 'deformations', 'animation', 'timeline', 'interaction'])
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
    props = module('properties', 'tools/benchmarks/properties-trace-report.py')
    anim = module('animation', 'tools/benchmarks/animation-trace-report.py')
    model = args.model.resolve()
    env0 = dict(os.environ)
    for key in list(env0):
        if key.startswith(('ZIPZI_BENCH', 'ZIPZI_WINDOW_BENCH', 'ZIPZI_FRAME_TRACE', 'ZIPZI_TIMELINE', 'ZIPZI_RUNTIME_INTERACTION', 'ZIPZI_DEV')) or key == 'MTL_HUD_ENABLED':
            env0.pop(key)
    for mode in args.modes:
        for pair in range(args.pairs):
            for side in ['baseline', 'candidate']:
                directory = OUT / f'{mode}-{pair + 1}-{side}'
                directory.mkdir(exist_ok=True)
                env = dict(env0)
                if mode == 'cpu':
                    cmd = [str(BIN / f'{side}-property-chain'), str(model), '60', '10', '12', 'all', 'retained']
                elif mode in ['timeline', 'interaction']:
                    env.update(ZIPZI_TIMELINE_MODEL=str(model), ZIPZI_TIMELINE_REQUIRE_ASSET='1', ZIPZI_TIMELINE_SAMPLES='60', ZIPZI_TIMELINE_OUTPUT=str(directory), ZIPZI_RUNTIME_INTERACTION_OUTPUT=str(directory), ZIPZI_RUNTIME_INTERACTION_SAMPLES='60')
                    cmd = [str(BIN / f'{side}-{mode}')]
                else:
                    env.update(ZIPZI_CONFIG_DIR=str(directory / 'config'), ZIPZI_FRAME_TRACE=str(directory / 'trace.jsonl'), ZIPZI_WINDOW_BENCH='1', ZIPZI_BENCH_MODEL=str(model), ZIPZI_BENCH_HZ='120', ZIPZI_BENCH_SAMPLES='120', ZIPZI_BENCH_WARMUP='10')
                    if mode == 'properties':
                        env.update(ZIPZI_BENCH_PROPERTIES='1', ZIPZI_BENCH_PROPERTY_CASE='opacity_drag', ZIPZI_BENCH_SAMPLES='30')
                    if mode == 'color':
                        env.update(ZIPZI_BENCH_PROPERTIES='1', ZIPZI_BENCH_PROPERTY_CASE='picker_rgb_0_drag', ZIPZI_BENCH_SAMPLES='30')
                    if mode == 'deformations':
                        env['ZIPZI_BENCH_DEFORMATIONS'] = '1'
                        if args.warp:
                            env['ZIPZI_BENCH_WARP'] = args.warp
                    if mode == 'animation':
                        env.update(ZIPZI_BENCH_ANIMATION='1', ZIPZI_BENCH_SAMPLES='600')
                    cmd = ['/usr/bin/caffeinate', '-d', '-i', str(BIN / f'{side}-window')]
                binary = pathlib.Path(cmd[-1] if mode not in ['cpu', 'timeline', 'interaction'] else cmd[0])
                fingerprint = hashlib.sha256(binary.read_bytes()).hexdigest()
                identity = {'binary_sha256': fingerprint, 'model': str(model), 'model_sha256': hashlib.sha256(model.read_bytes()).hexdigest()}
                identity['workload'] = {name: env.get(name) for name in ['ZIPZI_BENCH_PROPERTY_CASE', 'ZIPZI_BENCH_WARP', 'ZIPZI_BENCH_SAMPLES', 'ZIPZI_BENCH_WARMUP']}
                if (directory / 'summary.json').exists():
                    if not args.resume or json.loads((directory / 'identity.json').read_text()) != identity:
                        raise ValueError(f'refusing to reuse incompatible run: {directory}')
                    continue
                if (directory / 'trace.jsonl').exists() or (directory / 'samples.jsonl').exists():
                    raise ValueError(f'incomplete run exists: {directory}; preserve it and use a fresh output directory')
                (directory / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
                started = time.time()
                print('start', mode, pair + 1, side, flush=True)
                with (directory / 'run.log').open('w') as log, (directory / 'samples.jsonl').open('w') as samples:
                    subprocess.run(['/usr/bin/time', '-l', *cmd], cwd=ROOT, env=env, stdout=samples if mode == 'cpu' else log, stderr=log, check=True, timeout=600)
                if mode == 'cpu':
                    summary = chain.summarize([json.loads(l) for l in (directory / 'samples.jsonl').read_text().splitlines()])
                elif mode in ['timeline', 'interaction']:
                    summary = json.loads((directory / 'results.json').read_text())
                    assert summary['profile'] == 'release'
                else:
                    records = [json.loads(l) for l in (directory / 'trace.jsonl').read_text().splitlines()]
                    if mode in ['properties', 'color']:
                        _, summary = props.analyze(records)
                    elif mode == 'animation':
                        summary = anim.report(records)
                    else:
                        _, summary = frame.analyze(records)
                    if summary.get('issues'):
                        raise ValueError(summary['issues'])
                (directory / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
                print('done', mode, pair + 1, side, round(time.time() - started, 1), 'seconds', flush=True)


if __name__ == "__main__":
    main()
