#!/usr/bin/env python3
"""Collect the two tested Python builds into one validated publication set."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

from release import metadata


def collect(builds, output, revision, root):
    version = metadata(root)['python_version']
    selected = []
    for platform, suffix in (('macos', 'macosx_'), ('windows', 'win_amd64')):
        build = builds / platform
        report = json.loads((build / 'results.json').read_text(encoding='utf-8'))
        if report['status'] != 'passed' or report['base_commit'] != revision:
            raise ValueError(f'{platform} acceptance report is not for this commit')
        hashes = {Path(item['path'].replace('\\', '/')).name: item['sha256'] for item in report['artifacts']}
        wheels = list((build / 'wheels').glob(f'zipzi_native-{version}-*.whl'))
        if (len(wheels) != 1 or suffix not in wheels[0].name or 'cp314' not in wheels[0].name
                or (platform == 'macos' and 'arm64' not in wheels[0].name)):
            raise ValueError(f'{platform} native wheel is missing or has an unexpected tag')
        selected.extend(wheels)
        if platform == 'macos':
            for pattern in (f'zipzi-{version}.tar.gz', f'zipzi_native-{version}.tar.gz',
                            f'zipzi-{version}-py3-none-any.whl'):
                matches = list(build.rglob(pattern))
                if len(matches) != 1:
                    raise ValueError(f'Expected exactly one {pattern}')
                selected.extend(matches)
        for path in selected:
            if path.parent.parent != build:
                continue
            if hashes.get(path.name) != hashlib.sha256(path.read_bytes()).hexdigest():
                raise ValueError(f'{platform} artifact does not match acceptance report: {path.name}')
    output.mkdir(parents=True, exist_ok=False)
    for path in selected:
        shutil.copy2(path, output / path.name)
    print(f'PASS {len(selected)} tested Python distributions for {revision}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--builds', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    args = parser.parse_args()
    collect(args.builds, args.output, args.revision, Path(__file__).resolve().parents[2])


if __name__ == '__main__':
    main()
