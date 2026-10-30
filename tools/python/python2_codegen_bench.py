#!/usr/bin/env python3
"""Measure Python codegen changes; launch SDK probes with ./tools/dev-python.

Use --source for a frozen client/python directory from the comparison revision.
Preparation/native build time is outside the measured calls. Results are local
samples, including serialization fingerprints for comparison across revisions.
"""
import argparse
import gc
import hashlib
import json
from pathlib import Path
import statistics
import sys
import time
try:
    import resource
except ImportError:
    resource = None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=9)
    args = parser.parse_args()
    if args.rounds < 3:
        parser.error('--rounds must be at least 3')
    source = args.source or Path(__file__).resolve().parents[2] / 'crates/zipzi-python2/client/python'
    # Give both revisions the same ordinary package discovery route; comparing
    # a frozen directory against an editable import hook would skew cold import.
    sys.path.insert(0, str(source.resolve()))
    started = time.perf_counter()
    import zipzi
    imported = time.perf_counter() - started
    from zipzi import modeling as m, meshes
    from zipzi._generated import binary
    from zipzi._generated.codecs import bindings, commands, parameters

    uid = '00000000-0000-4000-8000-000000000001'
    appearance = m.scene.Appearance(opacity=1., multiply=(1., 1., 1.), screen=(0., 0., 0.))
    parameter = m.parameters.Parameter(id=uid, runtime_id='P', name='P', minimum=-1., maximum=1., default_value=0.)
    forms = {size: m.bindings.MeshKeyform(samples=(uid,), positions=tuple(m.common.Vec2(float(n), .5) for n in range(size)),
                                        appearance=appearance) for size in (3, 1000)}
    operations = {size: (m.commands.ModelingCommandSetMeshKeyform(id=uid, keyform=form),) for size, form in forms.items()}
    started = time.perf_counter_ns()
    first_json = commands.encode_ModelingCommand(operations[3][0])
    first_json_ns = time.perf_counter_ns() - started
    started = time.perf_counter_ns()
    commands.validate_ModelingCommand(operations[3][0])
    binary.encode_operations(operations[3])
    first_validate_binary_ns = time.perf_counter_ns() - started
    raw = {3: first_json, 1000: commands.encode_ModelingCommand(operations[1000][0])}
    rename = (meshes.Rename(uid, 'Face'),)

    cases = {
        'rename_binary': (lambda: binary.encode_operations(rename), 20000),
        'parameter_validate': (lambda: parameters.validate_Parameter(parameter), 20000),
    }
    fingerprints = {}
    for size in forms:
        def complete(size=size):
            commands.validate_ModelingCommand(operations[size][0])
            return binary.encode_operations(operations[size])
        cases.update({
            f'keyform_{size}_validate': (lambda size=size: bindings.validate_MeshKeyform(forms[size]), 2000 if size==3 else 40),
            f'keyform_{size}_json_encode': (lambda size=size: commands.encode_ModelingCommand(operations[size][0]), 2000 if size==3 else 40),
            f'keyform_{size}_json_read': (lambda size=size: commands.read_ModelingCommand(raw[size]), 1000 if size==3 else 20),
            f'keyform_{size}_validate_binary': (complete, 1000 if size==3 else 40),
        })
        bindings.validate_MeshKeyform(forms[size])
        assert commands.read_ModelingCommand(raw[size]) == operations[size][0]
        fingerprints[str(size)] = {
            'binary': hashlib.sha256(complete()).hexdigest(),
            'json': hashlib.sha256(json.dumps(raw[size], sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
        }
    for call, _ in cases.values():
        for _ in range(30):
            call()
    timings = {name: [] for name in cases}
    gc.disable()
    try:
        # Reverse the order every round to reduce systematic order effects.
        for index in range(args.rounds):
            names = list(cases) if index % 2 == 0 else list(reversed(cases))
            for name in names:
                call, count = cases[name]
                started = time.perf_counter_ns()
                for _ in range(count):
                    call()
                timings[name].append((time.perf_counter_ns() - started) / count)
    finally:
        gc.enable()
    rss = None if resource is None else resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if rss is not None and sys.platform != 'darwin':
        rss *= 1024
    result = {'python': sys.version, 'source': str(Path(zipzi.__file__).resolve()), 'import_seconds': imported,
              'peak_rss_bytes': rss,
              'cold_calls_ns': {'json_encode': first_json_ns, 'validate_binary': first_validate_binary_ns},
              'rounds': args.rounds, 'fingerprints': fingerprints,
              'timings_ns': {name: {'median': statistics.median(samples), 'samples': samples} for name, samples in timings.items()}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(args.output)


if __name__ == '__main__':
    main()
