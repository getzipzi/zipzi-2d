#!/usr/bin/env python3
"""Exercise compiled contract isolation and measure generation in a source copy.

Never edits the working checkout. Reports and copied build inputs belong in
target/, and each run requires a new output directory. No execution host is used.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = Path('crates/zipzi-python2')
IMPLEMENTATIONS = (
    ('core', 'crates/zipzi-core/src/draw_order.rs', '.copied().unwrap_or(1)', '.copied().unwrap_or({value})'),
    ('sdk', 'crates/zipzi-sdk/src/manual_mesh.rs', '[a.min(b), a.max(b)]', '[a.min(b).saturating_add({value}), a.max(b)]'),
    ('runtime', 'crates/zipzi-runtime/src/runtime/sampling.rs', '.fold(4096usize, |n, m| {', '.fold({value}usize, |n, m| {{'),
)


def snapshot(destination, source=ROOT):
    """Retain the real workspace graph; exclude machine config and build caches."""
    destination.mkdir()
    for name in ('Cargo.toml', 'Cargo.lock'):
        shutil.copy2(source / name, destination / name)
    ignore = shutil.ignore_patterns('target', '__pycache__', '.git', '.venv')
    for name in ('crates', 'vendor', 'third_party/purism-core'):
        shutil.copytree(source / name, destination / name, ignore=ignore)
    support = Path('tools/workspace/dev_platform.py')
    (destination / support).parent.mkdir(parents=True)
    shutil.copy2(source / support, destination / support)


def generate_instrumented(root, metrics, arguments):
    """Run the real entrypoint, recording phases without changing cache semantics."""
    spec = importlib.util.spec_from_file_location('acceptance_generate', root / PACKAGE / 'tools/generate.py')
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    timings = {}
    cargo = []
    original_run = gen.subprocess.run

    def run(command, *args, **kwargs):
        exporting = command[:2] == ['cargo', 'run']
        if exporting:
            command = ['--verbose' if arg == '--quiet' else arg for arg in command]
        result = original_run(command, *args, **kwargs)
        if exporting:
            cargo.extend(dict(state=state.lower(), package=package) for state, package in
                         re.findall(r'^\s*(Fresh|Compiling) ([\w-]+) ', result.stderr, re.M))
        return result

    def timed(name, function):
        def call(*args, **kwargs):
            started = time.perf_counter()
            try:
                return function(*args, **kwargs)
            finally:
                timings[name] = time.perf_counter() - started
        return call

    gen.subprocess.run = run
    for name in ('export', 'render', 'cached_render', 'write_outputs'):
        setattr(gen, name, timed(name, getattr(gen, name)))
    sys.argv = [str(root / PACKAGE / 'tools/generate.py'), *arguments]
    try:
        gen.main()
    finally:
        metrics.write_text(json.dumps(dict(phases=timings, cargo=cargo), indent=2) + '\n')


class Acceptance:
    def __init__(self, output, samples):
        self.output = output
        self.root = output / 'checkout'
        self.samples = samples
        self.runs = []
        snapshot(self.root)
        self.environment = dict(os.environ, CARGO_TARGET_DIR=str(output / 'external-target'), CARGO_BUILD_JOBS='2')

    def generate(self, scenario, *arguments, failure=None):
        index = len(self.runs)
        metrics = self.output / f'{index:02d}-{scenario}.json'
        command = [sys.executable, str(Path(__file__).resolve()), '_generate', str(self.root), str(metrics), *arguments]
        started = time.perf_counter()
        result = subprocess.run(command, cwd=self.root, env=self.environment, capture_output=True, text=True)
        elapsed = time.perf_counter() - started
        (self.output / f'{index:02d}-{scenario}.log').write_text(result.stdout + result.stderr)
        row = dict(scenario=scenario, seconds=elapsed, exit_code=result.returncode,
                   **json.loads(metrics.read_text()))
        self.runs.append(row)
        (self.output / 'runs.json').write_text(json.dumps(self.runs, indent=2) + '\n')
        if failure:
            assert result.returncode and failure in result.stderr, result.stderr[-6000:]
        else:
            assert result.returncode == 0, result.stderr[-6000:]
        return row

    def outputs(self):
        # Every generated file, including schema/reference, rather than schema alone.
        package = self.root / PACKAGE
        if not hasattr(self, '_targets'):
            spec = importlib.util.spec_from_file_location('acceptance_output_paths', package / 'tools/generate.py')
            gen = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(gen)
            self._targets = gen.TARGETS
        return {name: (package / name).read_bytes() for name in self._targets}

    def implementation_changes(self):
        baseline = self.outputs()
        for domain, name, before, after in IMPLEMENTATIONS:
            path = self.root / name
            original = path.read_text()
            assert before in original, name
            try:
                for attempt in range(self.samples):
                    # Change actual expression bytes every time; touching is insufficient.
                    value = attempt + (4097 if domain == 'runtime' else 2)
                    path.write_text(original.replace(before, after.format(value=value), 1))
                    row = self.generate(domain, '--check')
                    states = [p['state'] for p in row['cargo'] if p['package'] == 'zipzi-contract']
                    assert states and set(states) == {'fresh'}, row
                    assert not any(p['state'] == 'compiling' for p in row['cargo']), row
                    assert self.outputs() == baseline, domain
            finally:
                path.write_text(original)

    def drift(self, label, name, before, after):
        path = self.root / name
        original = path.read_text()
        assert before in original, (label, name)
        baseline = self.outputs()
        path.write_text(original.replace(before, after, 1))
        try:
            self.generate(label + '-drift', '--check', failure='Generated drift:')
            assert self.outputs() == baseline, '--check wrote generated files'
            self.generate(label + '-generate')
            assert self.outputs() != baseline, label
            self.generate(label + '-verified', '--check')
        finally:
            path.write_text(original)
            self.generate(label + '-restore')
            assert self.outputs() == baseline, label

    def contract_changes(self):
        name = 'crates/zipzi-contract/src/authoring/meshes.rs'
        before = '    pub triangles: Vec<[VertexId; 3]>,'
        self.drift('field', name, before, before + '\n    /// Acceptance probe.\n    pub contract_probe: u32,')
        path = self.root / name
        original = path.read_text()
        baseline = self.outputs()
        try:
            for attempt in range(self.samples):
                path.write_text(original.replace(before, before +
                    f'\n    /// Acceptance probe {attempt}.\n    pub contract_probe_{attempt}: u32,', 1))
                row = self.generate('field-build-generate')
                assert any(p == dict(state='compiling', package='zipzi-contract') for p in row['cargo']), row
                self.generate('field-build-verified', '--check')
        finally:
            path.write_text(original)
            self.generate('field-build-restore')
            assert self.outputs() == baseline
        self.drift('default', 'crates/zipzi-contract/src/authoring/meshes.rs',
                   'min_triangle_area: 0.0,', 'min_triangle_area: 0.5,')
        try:
            for attempt in range(self.samples):
                path.write_text(original.replace('min_triangle_area: 0.0,',
                                                 f'min_triangle_area: {attempt + 1}.0,', 1))
                self.generate('default-build-generate')
                self.generate('default-build-verified', '--check')
        finally:
            path.write_text(original)
            self.generate('default-build-restore')
            assert self.outputs() == baseline
        self.drift('enum', 'crates/zipzi-contract/src/model/appearance.rs',
                   'pub enum MeshConstraintKind {', 'pub enum MeshConstraintKind {\n    ContractProbe,')
        self.drift('operation-input', 'crates/zipzi-contract/src/runtime/snapshots.rs',
                   'pub struct MeshQuery {', 'pub struct MeshQuery {\n'
                   '        /// Acceptance input probe.\n        pub contract_probe: u32,')
        self.drift('operation-output', 'crates/zipzi-contract/src/runtime/snapshots.rs',
                   '    pub source: SourceRef,', '    pub source: SourceRef,\n'
                   '    /// Acceptance output probe.\n    pub contract_probe: u32,')

    def cache_and_failure(self):
        baseline = self.outputs()
        cache = self.root / 'target/python2/generate-cache.json'
        cache.unlink()
        self.generate('cache-deleted', '--check')
        self.generate('uncached', '--check', '--no-cache')
        assert self.outputs() == baseline
        # A successful cache must never hide a Rust compilation error.
        path = self.root / 'crates/zipzi-contract/src/operations.rs'
        original = path.read_text()
        try:
            path.write_text(original + '\ncompile_error!("acceptance invalid contract");\n')
            self.generate('invalid-contract', '--check', failure='acceptance invalid contract')
            assert self.outputs() == baseline
        finally:
            path.write_text(original)
        self.generate('failure-recovered', '--check')
        assert not (self.output / 'external-target').exists(), 'ambient Cargo target was used'

    def run(self):
        self.generate('cold', '--check')
        self.generate('warmup', '--check')
        for _ in range(self.samples):
            self.generate('unchanged', '--check')
        self.implementation_changes()
        self.contract_changes()
        self.cache_and_failure()
        summary = {}
        for scenario in dict.fromkeys(row['scenario'] for row in self.runs):
            rows = [r for r in self.runs if r['scenario'] == scenario]
            durations = [r['seconds'] for r in rows]
            summary[scenario] = dict(samples=len(rows), median=statistics.median(durations),
                                     minimum=min(durations), maximum=max(durations),
                                     phases={name: statistics.median(r['phases'].get(name, 0) for r in rows)
                                             for name in ('export', 'render', 'cached_render', 'write_outputs')})
        report = dict(status='passed', summary=summary, runs=self.runs,
                      python=sys.version, platform=sys.platform,
                      rustc=subprocess.check_output(['rustc', '--version'], text=True).strip(),
                      cargo=subprocess.check_output(['cargo', '--version'], text=True).strip())
        (self.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
        return report


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '_generate':
        generate_instrumented(Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4:])
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=5, help='Warm samples per scenario (default: 5)')
    args = parser.parse_args()
    if args.samples < 1:
        parser.error('--samples must be positive')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    Acceptance(output, args.samples).run()
    print(output / 'results.json')


if __name__ == '__main__':
    main()
