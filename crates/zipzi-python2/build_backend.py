"""Maturin backend with a self-contained SDK workspace in native sdists.

Maturin collects path dependencies but retains unrelated workspace patches.
Strip those desktop-only patches from the packaged manifest, resolve its lock
without changing selected dependency versions, and carry this backend at root.
Wheel builds delegate directly to Maturin and never run code generation.
"""
from pathlib import Path
import io
import re
import subprocess
import tarfile
import tempfile
import tomllib
import maturin


def __getattr__(name):
    return getattr(maturin, name)


def normalize_manifest(text):
    # These patch tables are for the Editor; no crate in the native SDK closure
    # depends on them. Keep all other workspace/package settings intact.
    desktop = {'gpui-fast', 'gpui-fast-macos', 'gpui-fast-apple', 'gpui-fast-windows', 'gpui-component'}
    def remove(match):
        tables = tomllib.loads(match[0])['patch']
        names = {name for table in tables.values() for name in table}
        if not names <= desktop:
            raise ValueError('Review SDK source packaging before adding non-desktop patches')
        return ''
    return re.sub(r'(?ms)^\[patch(?:\.[^\n]+)?\]\n.*?(?=^\[|\Z)', remove, text)


def locked_versions(path):
    return {(p['name'], p['version'], p.get('source')) for p in tomllib.loads(path.read_text(encoding='utf-8'))['package']}


def verify_codegen_sources(archive, checkout):
    """Every exporter/derive include and generator input must survive the sdist."""
    paths = [Path('tools/workspace/dev_platform.py')]
    for name in ('zipzi-contract', 'zipzi-api-derive'):
        package = Path('crates') / name
        if not (archive / package / 'Cargo.toml').is_file():
            raise RuntimeError(f'Source archive lacks {package}/Cargo.toml')
        paths.extend(path.relative_to(checkout) for path in (checkout / package / 'src').rglob('*.rs'))
    paths.extend(path.relative_to(checkout) for path in
                 (checkout / 'crates/zipzi-python2/tools').glob('generate*.py'))
    paths.extend(path.relative_to(checkout) for path in
                 (checkout / 'crates/zipzi-python2/tools/codegen').rglob('*.py'))
    for path in paths:
        if not (archive / path).is_file() or (archive / path).read_bytes() != (checkout / path).read_bytes():
            raise RuntimeError(f'Source archive lacks current codegen input: {path}')


def build_sdist(sdist_directory, config_settings=None):
    filename = maturin.build_sdist(sdist_directory, config_settings)
    destination = Path(sdist_directory) / filename
    with tempfile.TemporaryDirectory(prefix='zipzi-sdist-') as folder:
        folder = Path(folder)
        with tarfile.open(destination) as archive:
            archive.extractall(folder, filter='data')
        roots = list(folder.iterdir())
        if len(roots) != 1:
            raise RuntimeError('Expected one source archive root')
        root = roots[0]
        checkout = Path(__file__).resolve().parents[2]
        manifest = root / 'Cargo.toml'
        manifest.write_text(normalize_manifest(manifest.read_text(encoding='utf-8')))
        (root / 'build_backend.py').write_bytes(Path(__file__).read_bytes())
        # Keep the SDK README's repository-relative documentation links usable
        # after extraction, independently of the PyPI description.
        for relative in ('tools/README.md', 'docs/RELEASING.md', 'LICENSING.md'):
            document = root / relative
            document.parent.mkdir(parents=True, exist_ok=True)
            document.write_bytes((checkout / relative).read_bytes())
        (root / 'README.md').write_text(
            '# Zipzi native backend source\n\n'
            'Start with the [Python SDK README](crates/zipzi-python2/README.md) '
            'for installation, local documentation and source development.\n', encoding='utf-8')
        # Generation in the archive uses the same checkout-local lock primitive.
        support = root / 'tools/workspace/dev_platform.py'
        support.parent.mkdir(parents=True, exist_ok=True)
        support.write_bytes((checkout / 'tools/workspace/dev_platform.py').read_bytes())
        verify_codegen_sources(root, checkout)
        # Cargo preserves existing locked versions while removing the unused
        # desktop dependency graph. No build, generator or network is involved.
        versions = locked_versions(root / 'Cargo.lock')
        subprocess.run(['cargo', 'metadata', '--offline', '--format-version=1'],
                       cwd=root, check=True, stdout=subprocess.DEVNULL)
        if not locked_versions(root / 'Cargo.lock') <= versions:
            raise RuntimeError('Source packaging changed selected dependency versions')
        staged = folder / filename
        with tarfile.open(staged, 'w:gz') as archive:
            for path in sorted(root.rglob('*')):
                if path.is_file():
                    data = path.read_bytes()
                    info = tarfile.TarInfo(path.relative_to(folder).as_posix())
                    info.size, info.mode, info.mtime = len(data), 0o644, 0
                    archive.addfile(info, io.BytesIO(data))
        destination.write_bytes(staged.read_bytes())
    return filename
