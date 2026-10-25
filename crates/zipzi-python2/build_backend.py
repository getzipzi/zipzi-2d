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
    desktop = {'gpui-fast', 'gpui-fast-macos', 'gpui-fast-apple', 'gpui-component'}
    def remove(match):
        tables = tomllib.loads(match[0])['patch']
        names = {name for table in tables.values() for name in table}
        if not names <= desktop:
            raise ValueError('Review SDK source packaging before adding non-desktop patches')
        return ''
    return re.sub(r'(?ms)^\[patch(?:\.[^\n]+)?\]\n.*?(?=^\[|\Z)', remove, text)


def locked_versions(path):
    return {(p['name'], p['version'], p.get('source')) for p in tomllib.loads(path.read_text())['package']}


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
        manifest = root / 'Cargo.toml'
        manifest.write_text(normalize_manifest(manifest.read_text()))
        (root / 'build_backend.py').write_bytes(Path(__file__).read_bytes())
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
