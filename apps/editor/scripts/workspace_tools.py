"""Cargo artifact paths and bounded, single-repository benchmark provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE_PATHS = ('Cargo.toml', 'Cargo.lock', '.gitmodules', 'pyproject.toml', 'uv.lock',
                'modules', 'tests', 'tools', 'benchmarks/cubism-matrix/runners/kasane', 'apps/editor/crates',
                'apps/editor/scripts', 'apps/editor/assets', 'apps/editor/vendor')


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def executable(messages, name, kind='bin'):
    paths = set()
    for line in messages.splitlines():
        item = json.loads(line)
        target = item.get('target', {})
        if (item.get('reason') == 'compiler-artifact' and target.get('name') == name
                and kind in target.get('kind', []) and item.get('executable')):
            paths.add(Path(item['executable']))
    if len(paths) != 1:
        raise RuntimeError(f'Cargo did not report exactly one {name} executable')
    return paths.pop()


def source_manifest(root=ROOT):
    # Include new source files, but respect ignore rules and never walk model/cache trees.
    names = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *SOURCE_PATHS],
        cwd=root).decode().split('\0')
    files = {}
    for name in sorted(set(filter(None, names))):
        path = root / name
        if path.is_dir():  # gitlink; its identity is recorded in repository metadata
            continue
        if not path.resolve().is_relative_to(root.resolve()):
            raise ValueError(f'Source symlink escapes repository: {name}')
        if path.is_file():
            files[name] = digest(path)
    return files


def repository(root=ROOT):
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=root, text=True).strip()
    return {'head': git('rev-parse', 'HEAD'), 'status': git('status', '--short'),
            'submodules': git('submodule', 'status')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['executable', 'provenance'])
    parser.add_argument('input', type=Path)
    parser.add_argument('--model', type=Path)
    args = parser.parse_args()
    if args.command == 'executable':
        print(executable(args.input.read_text(), 'kasane-editor'))
    else:
        if args.model is None:
            parser.error('provenance requires --model')
        path = args.model.resolve(strict=True)
        args.input.write_text(json.dumps({
            'schema_version': 2, 'repository': repository(), 'sources': source_manifest(),
            'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=ROOT, text=True).strip(),
            'lock_sha256': digest(ROOT / 'Cargo.lock'),
            'model': str(path), 'model_files': {
                str(p.relative_to(path.parent)): digest(p) for p in sorted(path.parent.rglob('*'))
                if p.is_file() and p.name != '.kasane.lock'},
            'build_environment': {k: v for k, v in os.environ.items()
                                  if k in {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR'}
                                  or k.startswith('CARGO_PROFILE_')},
        }, indent=2) + '\n')


if __name__ == '__main__':
    main()
