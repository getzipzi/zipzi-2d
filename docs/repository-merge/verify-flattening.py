#!/usr/bin/env python3
"""Verify P3F provenance coverage, flat layout and preserved imported artifacts."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT).decode().strip()

mapping = json.loads((ROOT / 'docs/repository-merge/p3f-file-map.json').read_text())
rows = mapping['files']
original = {}
for line in git('ls-tree', '-r', mapping['base']).splitlines():
    metadata, path = line.split('\t')
    mode, kind, blob = metadata.split()
    original[path] = (mode, blob)
assert len(rows) == len(original)
assert {row['old'] for row in rows} == set(original)
for row in rows:
    assert (row['mode'], row['blob']) == original[row['old']], row['old']
    path = ROOT / row['new']
    assert path.exists(), row['new']
    assert path.is_symlink() == row['symlink'], row['new']
    if row['mode'] == '160000':
        assert git('ls-files', '-s', row['new']).split()[:2] == [row['mode'], row['blob']]
        continue
    if row['action'] == 'merge':
        assert row['reason'], row['old']
    # Preserve runtime assets, third-party code and immutable evidence byte-for-byte.
    immutable = (row['new'].startswith(('assets/', 'vendor/')) and row['new'] != 'vendor/README.md'
                 or '/archive/' in row['new'] or '/historical/' in row['new']
                 or row['new'] == 'docs/plans/sdk-runtime/r0-cpu-baseline.json')
    if immutable:
        assert git('hash-object', row['new']) == row['blob'], row['new']
    assert bool(path.stat().st_mode & 0o111) == (row['mode'] == '100755'), row['new']
for old in ('apps', 'modules', 'benchmarks/cubism-matrix/runners/kasane'):
    assert not (ROOT / old).exists(), old
for baseline in ('refs/merge-baseline/sdk', 'refs/merge-baseline/editor'):
    subprocess.run(['git', 'merge-base', '--is-ancestor', baseline, 'HEAD'], cwd=ROOT, check=True)
assert git('hash-object', 'Cargo.lock') == original['Cargo.lock'][1], 'Lockfile changed'
packages = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
assert len(packages['workspace_members']) == 18
for package in packages['packages']:
    assert Path(package['manifest_path']) == ROOT / 'crates' / package['name'] / 'Cargo.toml'
print(f"PASS: {len(rows)} source entries accounted for; 18 flat members; preserved assets/vendor/history/lock/gitlink")
