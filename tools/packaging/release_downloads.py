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
## 下载 / Downloads

只使用桌面编辑器，下载与你的系统对应的 **一项** 即可。

| 系统 / Platform | 下载 / Download | 安装 / Install |
| --- | --- | --- |
| macOS · Apple Silicon（M 系列） | [下载 Mac 安装包（DMG）]({link(mac)}) | 打开后拖入 Applications |
| Windows · Intel/AMD 64 位 | [下载 Windows 便携版（ZIP）]({link(windows)}) | 解压后运行 zipzi-editor.exe |

**Python SDK（CPython 3.14）**：通过 [PyPI](https://pypi.org/project/zipzi/{version}/) 安装，无需在附件里选择 wheel：

```sh
python -m pip install zipzi=={version}
```

使用 Python 本地运行模式 `zipzi.local()` 时，再安装 `zipzi-native=={version}`。

[完整源码（含子模块，开发者使用）]({link(source)}) · [SHA256 校验文件]({link('SHA256SUMS')})

下方的 Source code 是源码，不是桌面安装包。
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
