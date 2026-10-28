"""Native sdist keeps SDK dependencies pinned and excludes desktop-only patches."""
import importlib.util
import io
import os
import subprocess
import tarfile
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]

class Python2Distribution(unittest.TestCase):
    def backend(self):
        spec = importlib.util.spec_from_file_location('zipzi_build_backend', ROOT/'crates/zipzi-python2/build_backend.py')
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, maturin=types.ModuleType('maturin')):
            spec.loader.exec_module(module)
        return module

    def test_source_manifest_only_removes_reviewed_desktop_patches(self):
        backend = self.backend()
        original = (ROOT/'Cargo.toml').read_text()
        changed = backend.normalize_manifest(original)
        self.assertNotIn('[patch.', changed)
        self.assertIn('zipzi-runtime', changed)
        self.assertIn('[workspace.dependencies]', changed)
        self.assertEqual(backend.normalize_manifest(changed), changed)
        with self.assertRaises(ValueError):
            backend.normalize_manifest('[patch.crates-io]\nserde = { path = "vendor/serde" }\n')

    def test_pinned_versions_include_source_identity_and_ignore_checksums(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder)/'Cargo.lock'
            lock.write_text('version = 4\n[[package]]\nname="test"\nversion="1.2.3"\nsource="registry+https://example.test"\nchecksum="fixture"\n')
            self.assertEqual(self.backend().locked_versions(lock), {('test','1.2.3','registry+https://example.test')})



TOOLS=Path(__file__).resolve().parents[2]/'crates/zipzi-python2/tools'
sys.path.insert(0,str(TOOLS))
SPEC=importlib.util.spec_from_file_location('python2_distribution',TOOLS/'distribution.py')
distribution=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(distribution)
from _support import publish_executable

class Distribution(unittest.TestCase):
    def test_host_publication_keeps_old_version_and_recovers_copy_failure(self):
        with tempfile.TemporaryDirectory(prefix='host 中文 ') as folder:
            root = Path(folder)
            source = root / 'fixture.exe'
            source.write_bytes(b'old host')
            first = publish_executable(root / 'output', source)
            with first.open('rb') as old:
                self.assertEqual(publish_executable(root / 'output', source), first)
                source.write_bytes(b'new host')
                with patch('_support.shutil.copy2', side_effect=OSError('copy failed')):
                    with self.assertRaises(OSError):
                        publish_executable(root / 'output', source)
                second = publish_executable(root / 'output', source)
                self.assertNotEqual(first, second)
                self.assertEqual(second.read_bytes(), b'new host')
                self.assertEqual(old.read(), b'old host')
            self.assertEqual(list((root / 'output').rglob('.host-*')), [])

    def test_reused_cargo_cache_observes_changed_zero_timestamp_archive_dependencies(self):
        with tempfile.TemporaryDirectory(prefix='zipzi-archive-cache-') as temporary:
            root=Path(temporary)
            for attempt,method,value in ((1,'first',1),(2,'newer',2)):
                folder=root/str(attempt);folder.mkdir();archive=folder/'fixture-0.1.0.tar.gz'
                files={'Cargo.toml':'[workspace]\nmembers=["core","app"]\nresolver="3"\n',
                    'core/Cargo.toml':'[package]\nname="distribution_fixture"\nversion="0.1.0"\nedition="2021"\n',
                    'core/src/lib.rs':f'pub fn {method}() -> u32 {{{value}}}\n',
                    'app/Cargo.toml':'[package]\nname="distribution-check"\nversion="0.1.0"\nedition="2021"\n[dependencies]\ndistribution_fixture={path="../core"}\n',
                    'app/src/main.rs':f'fn main() {{println!("{{}}",distribution_fixture::{method}());}}\n'}
                with tarfile.open(archive,'w:gz') as bundle:
                    for name,text in files.items():
                        data=text.encode();info=tarfile.TarInfo('fixture-0.1.0/'+name)
                        info.size=len(data);info.mtime=0;bundle.addfile(info,io.BytesIO(data))
                source=distribution.extract(archive,root/'source')
                # Both requests use identical package names, versions, lengths and a shared cache.
                result=subprocess.run(['cargo','run','--offline','--quiet','-p','distribution-check'],cwd=source,
                    env=dict(os.environ,CARGO_TARGET_DIR=str(root/'cargo-target')),capture_output=True,text=True,timeout=60)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                self.assertEqual(result.stdout.strip(),str(value),'Cargo reused a previous archive result')



if __name__=='__main__':unittest.main()
