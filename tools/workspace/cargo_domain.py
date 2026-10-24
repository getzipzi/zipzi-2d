#!/usr/bin/env python3
"""Run Cargo against an explicit, exhaustive repository package domain."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / 'tools/workspace/package-domains.json'


def domains():
    groups = json.loads(CONFIG.read_text())
    if set(groups) != {'sdk', 'editor'}:
        raise ValueError('Expected sdk and editor domains')
    names = [name for group in groups.values() for name in group]
    if not names or len(names) != len(set(names)):
        raise ValueError('Package domains must be nonempty and disjoint')
    if any(not group for group in groups.values()):
        raise ValueError('Empty package domain')
    return groups


def validate(groups):
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
    members = {p['name'] for p in metadata['packages'] if p['id'] in metadata['workspace_members']}
    configured = {name for group in groups.values() for name in group}
    if members != configured:
        raise ValueError(f'Unassigned packages: {sorted(members - configured)}; stale packages: {sorted(configured - members)}')
    print(f"PASS: {len(groups['sdk'])} SDK + {len(groups['editor'])} Editor packages; complete, disjoint workspace coverage")


def command(groups, domain, operation, args):
    if operation not in {'test', 'check', 'build', 'clippy', 'fmt'}:
        raise ValueError(f'Unsupported Cargo operation: {operation}')
    cargo_args = args[:args.index('--')] if '--' in args else args
    if any(arg in {'--workspace', '--all', '--exclude', '-p', '--package', '--manifest-path'}
           or arg.startswith(('--package=', '--exclude=', '--manifest-path='))
           or arg.startswith('-p') for arg in cargo_args):
        raise ValueError('Package selection is owned by package-domains.json')
    return ['cargo', operation, *[arg for name in groups[domain] for arg in ('-p', name)], *args]


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    groups = domains()
    if args == ['--check']:
        validate(groups)
        return 0
    if len(args) < 2 or args[0] not in groups:
        raise ValueError('Usage: cargo_domain.py --check | sdk|editor test|check|build|clippy|fmt [arguments]')
    return subprocess.call(command(groups, args[0], args[1], args[2:]), cwd=ROOT)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(error, file=sys.stderr)
        raise SystemExit(2)
