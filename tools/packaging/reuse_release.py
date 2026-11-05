#!/usr/bin/env python3
"""Reuse a completed default-branch release build for the exact tagged commit."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from urllib.parse import urlencode

ARTIFACTS = frozenset({'alpha-release-assets', 'alpha-python-client',
                      'alpha-python-native', 'alpha-release-manifest'})


def github_api(path):
    result = subprocess.run(['gh', 'api', path], check=True, capture_output=True,
                            text=True, encoding='utf-8')
    return json.loads(result.stdout)


def manifest_artifact(configuration=''):
    return 'alpha-release-manifest' + (f'-{configuration}' if configuration else '')


def find_build(repository, revision, branch, api=github_api, configuration=''):
    query = urlencode(dict(head_sha=revision, status='success', per_page=20))
    runs = api(f'repos/{repository}/actions/workflows/release-alpha.yml/runs?{query}')
    for run in runs['workflow_runs']:
        if (run.get('head_sha') != revision or run.get('head_branch') != branch
                or (run.get('head_repository') or {}).get('full_name') != repository
                or run.get('event') not in {'push', 'workflow_dispatch'}
                or run.get('status') != 'completed' or run.get('conclusion') != 'success'
                or type(run.get('id')) is not int or run['id'] <= 0):
            continue
        artifacts = api(f'repos/{repository}/actions/runs/{run["id"]}/artifacts?per_page=100')
        available = {item['name'] for item in artifacts['artifacts']
                     if item.get('expired') is False}
        required = (ARTIFACTS - {'alpha-release-manifest'}) | {manifest_artifact(configuration)}
        if required <= available:
            return str(run['id'])
    return ''


def file_hashes(directory):
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(directory.iterdir()) if path.is_file()}


def write_manifest(path, assets, revision, tag, configuration=''):
    files = file_hashes(assets)
    if 'SHA256SUMS' not in files:
        raise ValueError('Release checksum file is missing')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(dict(schema=1, revision=revision, tag=tag, files=files,
                                   configuration=configuration),
                               indent=2) + '\n', encoding='utf-8')


def verify_manifest(path, assets, distributions, revision, tag, configuration=''):
    manifest = json.loads(path.read_text(encoding='utf-8'))
    if (manifest.get('schema') != 1 or manifest.get('revision') != revision
            or manifest.get('tag') != tag or manifest.get('configuration', '') != configuration):
        raise ValueError('Reusable build does not match the release commit and version')
    hashes = file_hashes(assets)
    if 'SHA256SUMS' not in hashes or hashes != manifest.get('files'):
        raise ValueError('Release assets differ from the validated build manifest')
    python_hashes = {name: digest for name, digest in hashes.items()
                     if name.startswith(('zipzi-', 'zipzi_native-'))
                     and name.endswith(('.whl', '.tar.gz'))}
    if not python_hashes or file_hashes(distributions) != python_hashes:
        raise ValueError('PyPI distributions differ from the validated release assets')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    find = commands.add_parser('find')
    find.add_argument('--repository', required=True)
    find.add_argument('--branch', required=True)
    find.add_argument('--revision', required=True)
    find.add_argument('--github-output', type=Path, required=True)
    find.add_argument('--configuration', default='')
    for name in ('write', 'verify'):
        command = commands.add_parser(name)
        command.add_argument('--manifest', type=Path, required=True)
        command.add_argument('--assets', type=Path, required=True)
        command.add_argument('--revision', required=True)
        command.add_argument('--tag', required=True)
        command.add_argument('--configuration', default='')
        if name == 'verify':
            command.add_argument('--distributions', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'find':
        try:
            run_id = find_build(args.repository, args.revision, args.branch,
                                configuration=args.configuration)
        except (subprocess.CalledProcessError, KeyError, ValueError) as error:
            # A lookup failure must fall back to a complete build, never to
            # publishing artifacts from an unverified candidate.
            print(f'Build lookup unavailable ({type(error).__name__}); building normally.')
            run_id = ''
        with args.github_output.open('a', encoding='utf-8') as output:
            output.write(f'run_id={run_id}\n')
        print(f'Reusing validated build {run_id}' if run_id else 'No reusable build; building normally.')
    elif args.command == 'write':
        write_manifest(args.manifest, args.assets, args.revision, args.tag, args.configuration)
    else:
        verify_manifest(args.manifest, args.assets, args.distributions, args.revision, args.tag,
                        args.configuration)
        print('PASS release manifest, commit, version and all publication file hashes')


if __name__ == '__main__':
    main()
