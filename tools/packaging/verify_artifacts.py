#!/usr/bin/env python3
"""Verify local workspace source, wheel and macOS app distribution contents."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
EDITOR_ASSETS = ROOT/'crates/zipzi-editor/assets'


def check_notices():
    for name in ['LICENSE-MIT', 'THIRD_PARTY_NOTICES.md']:
        assert (ROOT/'crates/zipzi-python'/name).read_bytes() == (ROOT/name).read_bytes(), f'Stale Python distribution notice: {name}'


def check_wheel(path):
    with zipfile.ZipFile(path) as wheel:
        names = wheel.namelist()
        assert len(names) == len(set(names))
        for name in ['LICENSE-MIT', 'THIRD_PARTY_NOTICES.md']:
            matches = [n for n in names if n.endswith('.dist-info/licenses/'+name)]
            assert len(matches) == 1 and wheel.read(matches[0]) == (ROOT/name).read_bytes(), name
        metadata = next(n for n in names if n.endswith('.dist-info/METADATA'))
        assert b'Name: zipzi' in wheel.read(metadata).splitlines()
        assert b'License-Expression: MIT' in wheel.read(metadata)
        assert 'zipzi/py.typed' in names and 'zipzi/_native.pyi' in names
        native = [n for n in names if n.startswith('zipzi/_native') and n.endswith(('.so', '.pyd'))]
        assert len(native) == 1, native
        assert not any(n.startswith(('models/', 'assets/', 'crates/zipzi-editor/', 'vendor/')) for n in names)
    print(f'PASS wheel: {path.name}; MIT/notices, typing and native extension')


def check_app(path):
    resources = path/'Contents/Resources'
    plist = plistlib.loads((path/'Contents/Info.plist').read_bytes())
    assert plist['CFBundleIdentifier'] == 'dev.zipzi.editor'
    assert plist['CFBundleName'] == 'Zipzi Editor'
    assert plist['CFBundleExecutable'] == 'zipzi-editor'
    binary = path/'Contents/MacOS'/plist['CFBundleExecutable']
    assert binary.is_file() and binary.stat().st_mode & 0o111
    data = binary.read_bytes()
    for name in ['LICENSE-MIT', 'LICENSE-GPL-3.0', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md']:
        assert (resources/name).read_bytes() == (ROOT/name).read_bytes(), name
    assert (resources/'LICENSE').read_bytes() == (ROOT/'LICENSE-GPL-3.0').read_bytes()
    assert (resources/'AppIcon.icns').read_bytes() == (EDITOR_ASSETS/'app-icons/zipzi-editor-icon-vector.icns').read_bytes()
    for path in (EDITOR_ASSETS/'icons').glob('*.svg'):
        assert path.read_bytes() in data, f'Missing embedded icon: {path.name}'
    for path in (EDITOR_ASSETS/'locales').glob('*.toml'):
        catalog = tomllib.loads(path.read_text())
        assert catalog['app']['description'].encode() in data, f'Missing embedded locale: {path.name}'
    for path in (ROOT/'vendor').rglob('*'):
        if path.is_file() and path.name.lower().startswith(('license', 'copying', 'notice')):
            assert (resources/'vendor-notices'/path.relative_to(ROOT/'vendor')).read_bytes() == path.read_bytes()
    print(f'PASS app: {binary}; licenses/notices, bundle icon, embedded icons and both locales')


def check_source(path):
    with tarfile.open(path) as archive:
        names = archive.getnames()
        assert len(names) == len(set(names))
        assert not any('/models/local/' in n or '/target/' in n or '/.git/' in n or '/__pycache__/' in n
                       or '/third_party/CubismSdkForNative' in n
                       or '/third_party/CubismNativeFramework/' in n
                       or n == 'zipzi-2d/.cargo/config.toml' for n in names)
        manifest = json.load(archive.extractfile('zipzi-2d/SOURCE-MANIFEST.json'))
        for name, digest in manifest['sha256'].items():
            member = archive.getmember('zipzi-2d/'+name)
            assert member.isfile(), name
            assert hashlib.sha256(archive.extractfile(member).read()).hexdigest() == digest, name
        for name in ['LICENSE-MIT','LICENSE-GPL-3.0','THIRD_PARTY_NOTICES.md','Cargo.toml','Cargo.lock',
                     'third_party/purism-core/include/PurismCore.h',
                     'crates/zipzi-editor/assets/app-icons/zipzi-editor-icon-vector.icns',
                     'crates/zipzi-editor/assets/icons/bezier-curve.svg',
                     'crates/zipzi-editor/assets/icons/origin-crosshair.svg',
                     'crates/zipzi-editor/assets/locales/en.toml',
                     'crates/zipzi-editor/assets/locales/zh-CN.toml']:
            assert 'zipzi-2d/'+name in names, name
        for crate in (ROOT/'crates').iterdir():
            if (crate/'Cargo.toml').exists():assert 'zipzi-2d/crates/'+crate.name+'/Cargo.toml' in names
    print(f'PASS source: {len(manifest["sha256"])} hashed files; root layout, assets, patches, submodule, licenses')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wheel', type=Path)
    parser.add_argument('--app', type=Path)
    parser.add_argument('--source', type=Path)
    args = parser.parse_args()
    check_notices()
    for path, check in [(args.wheel, check_wheel), (args.app, check_app), (args.source, check_source)]:
        if path:check(path)


if __name__ == '__main__':
    main()
