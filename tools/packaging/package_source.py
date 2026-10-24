#!/usr/bin/env python3
"""Archive the workspace layout, including initialized gitlinks, without local models/caches."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]


def source_files(root=ROOT):
    names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root).decode().split('\0')
    files = {}
    for name in sorted(set(filter(None, names))):
        path = root / name
        if not path.exists():
            raise ValueError(f'Missing tracked source: {name}')
        if path.is_dir():
            if not (path / '.git').exists():
                raise ValueError(f'Initialize source submodule before packaging: {name}')
            nested = subprocess.check_output(['git', 'ls-files', '-z'], cwd=path).decode().split('\0')
            candidates = [(name + '/' + n, path / n) for n in nested if n]
        else:
            candidates = [(name, path)]
        for relative, source in candidates:
            if not source.resolve().is_relative_to(root.resolve()):
                raise ValueError(f'Source symlink escapes repository: {relative}')
            if source.is_dir():
                raise ValueError(f'Nested gitlink must be explicitly handled: {relative}')
            if relative.startswith(('models/local/', 'target/', '.git/',
                                    'third_party/CubismSdkForNative',
                                    'third_party/CubismNativeFramework/')) or '__pycache__' in Path(relative).parts or relative == '.cargo/config.toml' or (Path(relative).name.startswith('_native') and Path(relative).suffix in {'.so', '.pyd'}):
                raise ValueError(f'Private/generated content tracked: {relative}')
            files[relative] = source
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    files = source_files()
    if args.output.resolve() in [p.resolve() for p in files.values()]:
        parser.error('Output must be outside the source set (for example target/releases/)')
    manifest = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'status': subprocess.check_output(['git', 'status', '--short'], cwd=ROOT, text=True),
                'sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in files.items()}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(args.output, 'w:gz') as archive:
        for name, path in files.items():
            archive.add(path, arcname='zipzi-2d/' + name, recursive=False)
        data = (json.dumps(manifest, indent=2) + '\n').encode()
        info = tarfile.TarInfo('zipzi-2d/SOURCE-MANIFEST.json')
        info.size = len(data)
        archive.addfile(info, io.BytesIO(data))
    print(f'{args.output}: {len(files)} source files, with hashes and checkout status')


if __name__ == '__main__':
    main()
