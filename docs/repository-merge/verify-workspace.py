#!/usr/bin/env python3
"""P2 structural acceptance; requires Python 3.11+ and Cargo's dependency cache."""
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
EDITOR = ROOT / 'apps/editor'

def metadata(cwd, *options):
    return json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--features', 'kasane-editor/debug-tools,kasane-editor/test-support', *options], cwd=cwd))

root = metadata(ROOT)
app = metadata(EDITOR)
assert root == app, 'Root and Editor resolve different workspaces'
assert Path(root['workspace_root']) == ROOT
assert Path(root['target_directory']) == ROOT / 'target'
assert not (EDITOR / 'Cargo.toml').exists()
assert not (EDITOR / 'Cargo.lock').exists()
packages = {p['id']: p for p in root['packages']}
members = {packages[i]['name']: packages[i] for i in root['workspace_members']}
baseline = json.loads((ROOT / 'docs/repository-merge/baseline.json').read_text())
assert set(members) == set(baseline['sdk']['workspace_packages'] + baseline['editor']['workspace_packages'])
assert len(members) == 18
assert {packages[i]['name'] for i in root['workspace_default_members']} == {'kasane-core', 'kasane-project', 'kasane-sdk'}
editor_names = {'kasane-editor', 'editor-core', 'editor-render'}
for name, p in members.items():
    assert p['edition'] == ('2024' if name in editor_names else '2021')
    assert p['license'] == ('GPL-3.0-only' if name in editor_names else 'MIT')
    assert p['rust_version'] == '1.95'
for p in packages.values():
    if p['source'] is None:
        assert Path(p['manifest_path']).is_relative_to(ROOT), p['manifest_path']
for name, path in {
    'gpui-fast': 'gpui-fast', 'gpui-fast-macos': 'gpui-fast-macos',
    'gpui-fast-apple': 'gpui-fast-apple', 'gpui-component': 'gpui-kit/crates/component',
}.items():
    matching = [p for p in packages.values() if p['name'] == name]
    assert len(matching) == 1
    assert Path(matching[0]['manifest_path']) == EDITOR / 'vendor' / path / 'Cargo.toml'
    assert matching[0]['id'] not in root['workspace_members']
for name in ['gpui-kit', 'gpui-fps']:
    matching = [p for p in packages.values() if p['name'] == name]
    assert matching and all('507e82b8115ac73af92b7d3856501f9d7b73b2c9' in p['source'] for p in matching)
nodes = {n['id']: n for n in root['resolve']['nodes']}
for name in baseline['sdk']['workspace_packages']:
    seen = set()
    pending = [members[name]['id']]
    while pending:
        i = pending.pop()
        if i in seen:
            continue
        seen.add(i)
        p = packages[i]
        assert p['name'] not in editor_names and not p['name'].startswith('gpui'), (name, p['name'])
        pending.extend(nodes[i]['dependencies'])
manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())
assert manifest['workspace']['resolver'] == '3'
assert 'rc' in manifest['workspace']['dependencies']['serde']['features']
assert 'float_roundtrip' in manifest['workspace']['dependencies']['serde_json']['features']
profile = manifest['profile']
assert profile['release']['package']['kasane-python']['strip'] == 'none'
assert profile['dev']['debug'] == 'line-tables-only'
assert profile['dev']['incremental'] is True
assert profile['dev']['package']['*']['opt-level'] == 3
assert all(p['opt-level'] == 0 for name, p in profile['dev']['package'].items() if name != '*')
print('PASS: 18 members; shared root/lock/target; editions/licenses; patches/revision; SDK dependency boundary; profiles/features')
