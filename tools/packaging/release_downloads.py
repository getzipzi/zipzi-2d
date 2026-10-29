#!/usr/bin/env python3
"""Prepare desktop downloads and automatic installation links for GitHub Releases."""
import argparse
import hashlib
from pathlib import Path
import shutil
from urllib.parse import quote

from release import versions

START = '<!-- zipzi-downloads:start -->'
END = '<!-- zipzi-downloads:end -->'


def download_names(tag):
    if not tag.startswith('v'):
        raise ValueError('Expected a version tag starting with v')
    versions(tag[1:])
    return (f'Zipzi-Editor-{tag}-macos-arm64.dmg',
            f'Zipzi-Editor-{tag}-windows-x86_64.zip',
            f'Zipzi-2D-{tag}-source.tar.gz')


def prepare_downloads(source, output, tag):
    names = download_names(tag)
    for name in names:
        if not (source / name).is_file():
            raise ValueError(f'Missing release download: {name}')
    output.mkdir(parents=True, exist_ok=False)
    checksums = []
    for name in names:
        shutil.copy2(source / name, output / name)
        checksums.append(f'{hashlib.sha256((output / name).read_bytes()).hexdigest()}  {name}\n')
    (output / 'SHA256SUMS').write_text(''.join(checksums), encoding='utf-8')


def download_notes(repository, tag):
    mac, windows, source = download_names(tag)
    version = versions(tag[1:])['python_version']
    base = f'https://github.com/{repository}/releases/download/{quote(tag, safe="")}'
    def link(name):
        return f'{base}/{quote(name, safe="")}'
    return f'''{START}
## Downloads

To use the desktop editor, download the package for your platform.

| Platform | Download | Installation |
| --- | --- | --- |
| macOS · Apple Silicon (M series) | [Download for macOS (DMG)]({link(mac)}) | Open the DMG and drag the app to Applications. |
| Windows · Intel/AMD 64-bit | [Download for Windows (ZIP)]({link(windows)}) | Extract the archive and run `zipzi-editor.exe`. |

**Python SDK (CPython 3.14):** Install from [PyPI](https://pypi.org/project/zipzi/{version}/):

```sh
python -m pip install zipzi=={version}
```

For local execution with `zipzi.local()`, also install `zipzi-native=={version}`.

[Complete source archive (including submodules)]({link(source)}) · [SHA-256 checksums]({link('SHA256SUMS')})

The automatically generated source code archives below do not contain desktop installers.
{END}
'''


def prepend_notes(existing, repository, tag):
    if START in existing and END in existing:
        before, rest = existing.split(START, 1)
        _, after = rest.split(END, 1)
        existing = before + after
    return download_notes(repository, tag) + '\n' + existing.strip() + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--notes', type=Path, required=True)
    parser.add_argument('--repository', required=True)
    parser.add_argument('--tag', required=True)
    args = parser.parse_args()
    prepare_downloads(args.input, args.output, args.tag)
    args.notes.parent.mkdir(parents=True, exist_ok=True)
    args.notes.write_text(download_notes(args.repository, args.tag), encoding='utf-8')


if __name__ == '__main__':
    main()
