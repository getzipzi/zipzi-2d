#!/usr/bin/env python3
"""Synchronize release versions and produce metadata without publishing packages."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
VERSION = re.compile(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.([1-9]\d*))?')


def versions(version):
    match = VERSION.fullmatch(version)
    if not match:
        raise ValueError('Expected X.Y.Z or X.Y.Z-alpha.N / beta.N / rc.N')
    major, minor, patch, stage, number = match.groups()
    base = f'{major}.{minor}.{patch}'
    suffix = {'alpha': 'a', 'beta': 'b', 'rc': 'rc'}.get(stage, '')
    return dict(version=version, python_version=base + suffix + (number or ''),
                tag='v' + version, app_version=base, prerelease=bool(stage))


def workspace(root=ROOT):
    return tomllib.loads((root / 'Cargo.toml').read_text())['workspace']


def metadata(root=ROOT, tag=None):
    result = versions(workspace(root)['package']['version'])
    if tag is not None and tag != result['tag']:
        raise ValueError(f'Tag {tag!r} does not match manifest {result["tag"]!r}')
    return result


def members(root=ROOT):
    return {tomllib.loads((root / path / 'Cargo.toml').read_text())['package']['name']:
            root / path / 'Cargo.toml' for path in workspace(root)['members']}


def check(root=ROOT, tag=None):
    info = metadata(root, tag)
    client = tomllib.loads((root / 'crates/zipzi-python2/client/pyproject.toml').read_text())['project']
    native = tomllib.loads((root / 'crates/zipzi-python2/pyproject.toml').read_text())['project']
    if client['name'] != 'zipzi' or native['name'] != 'zipzi-native':
        raise ValueError('Public Python distributions must be zipzi and zipzi-native')
    if client['version'] != info['python_version'] or f'zipzi=={info["python_version"]}' not in native['dependencies']:
        raise ValueError('Python release versions do not match the workspace')
    owned = members(root)
    tables = [workspace(root)['dependencies']]
    for manifest in owned.values():
        data = tomllib.loads(manifest.read_text())
        for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
            tables.append(data.get(section, {}))
    for table in tables:
        for name, dependency in table.items():
            if name in owned and 'path' in dependency and dependency.get('version') != '=' + info['version']:
                raise ValueError(f'{name}: internal path dependency needs the exact release version')
    locked = tomllib.loads((root / 'Cargo.lock').read_text())['package']
    for name in owned:
        packages = [p for p in locked if p['name'] == name and 'source' not in p]
        if len(packages) != 1 or packages[0]['version'] != info['version']:
            raise ValueError(f'{name}: Cargo.lock release version is stale')
    return info


def set_version(version, root=ROOT):
    info = versions(version)
    owned = members(root)
    manifest = root / 'Cargo.toml'
    text = re.sub(r'(\[workspace\.package\]\s*\nversion = ")[^"]+("\n)',
                  lambda m: m[1] + version + m[2], manifest.read_text(), count=1)
    manifest.write_text(text)
    # Local builds use paths; packaged crates resolve these exact registry versions.
    for manifest in [root / 'Cargo.toml', *owned.values()]:
        text = manifest.read_text()
        for name in owned:
            pattern = rf'(?m)^({re.escape(name)}\s*=\s*\{{)([^\n]*\bpath\s*=[^\n]*)(\}})'
            def update(match):
                fields = re.sub(r'\s*version\s*=\s*"[^"]+"\s*,?', '', match[2]).strip()
                return match[1] + f' version = "={version}", ' + fields + ' ' + match[3]
            text = re.sub(pattern, update, text)
        manifest.write_text(text)
    client = root / 'crates/zipzi-python2/client/pyproject.toml'
    client.write_text(re.sub(r'(?m)^version = "[^"]+"', f'version = "{info["python_version"]}"', client.read_text()))
    native = root / 'crates/zipzi-python2/pyproject.toml'
    native.write_text(re.sub(r'zipzi==[^"\s]+', f'zipzi=={info["python_version"]}', native.read_text()))
    lock = root / 'Cargo.lock'
    blocks = lock.read_text().split('[[package]]')
    for index, block in enumerate(blocks[1:], 1):
        data = tomllib.loads(block)
        if data.get('name') in owned and 'source' not in data:
            blocks[index] = re.sub(r'(?m)^version = "[^"]+"', f'version = "{version}"', block, count=1)
    lock.write_text('[[package]]'.join(blocks))
    return check(root)


def cargo_order(root=ROOT):
    """Only public Rust SDK libraries; Python adapters are distributed as wheels."""
    packages = {name: tomllib.loads(path.read_text()) for name, path in members(root).items()}
    public = {name for name, data in packages.items()
              if data['package'].get('publish', True) is not False
              and name != 'zipzi-python2'}
    pending = {name: set(packages[name].get('dependencies', {})) & public for name in public}
    order = []
    while pending:
        ready = sorted(name for name, dependencies in pending.items() if not dependencies)
        if not ready:
            raise ValueError('Public SDK dependency cycle: ' + ', '.join(sorted(pending)))
        order.extend(ready)
        pending = {name: dependencies - set(ready) for name, dependencies in pending.items() if name not in ready}
    return order


def app_plist(output, root=ROOT):
    info = metadata(root)
    build = os.environ.get('ZIPZI_BUILD_NUMBER', '1')
    if not re.fullmatch(r'[1-9]\d*(?:\.\d+){0,2}', build):
        raise ValueError('ZIPZI_BUILD_NUMBER must be a positive numeric build version')
    revision = os.environ.get('GITHUB_SHA')
    if not revision:
        revision = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=root,
                                  capture_output=True, text=True).stdout.strip() or 'unknown'
    data = dict(CFBundleIdentifier='dev.zipzi.editor', CFBundleName='Zipzi Editor',
                CFBundleExecutable='zipzi-editor', CFBundlePackageType='APPL',
                CFBundleShortVersionString=info['app_version'], CFBundleVersion=build,
                ZipziReleaseVersion=info['version'], ZipziSourceRevision=revision,
                CFBundleIconFile='AppIcon', NSHighResolutionCapable=True)
    output.write_bytes(plistlib.dumps(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('check', 'metadata'):
        command = commands.add_parser(name)
        command.add_argument('--tag')
        command.add_argument('--github-output', type=Path)
    commands.add_parser('set').add_argument('version')
    commands.add_parser('cargo-plan')
    commands.add_parser('app-plist').add_argument('output', type=Path)
    args = parser.parse_args()
    try:
        if args.command == 'app-plist':
            app_plist(args.output)
            return
        if args.command == 'cargo-plan':
            check()
            print('\n'.join(cargo_order()))
            return
        if args.command == 'set':
            info = set_version(args.version)
        else:
            info = (check if args.command == 'check' else metadata)(tag=args.tag)
            if args.github_output:
                with args.github_output.open('a') as output:
                    for key, value in info.items():
                        output.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')
        print(json.dumps(info, indent=2))
    except (ValueError, KeyError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
