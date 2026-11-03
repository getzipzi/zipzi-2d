"""Real loaded-extension publication. Run through tools/dev.py python."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import zipzi_native._native as native

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('source_launcher', ROOT / 'tools/python/dev_python.py')
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)

CHILD = '''
import site,sys,time
from pathlib import Path
site.addsitedir(sys.argv[1])
import zipzi,zipzi_native._native
with zipzi.local() as runtime:
    assert len(runtime.sessions.list()) == 0
    Path(sys.argv[2]).write_text(zipzi_native._native.__file__, encoding='utf-8')
    if len(sys.argv) > 3:
        deadline=time.monotonic()+30
        while not Path(sys.argv[3]).exists():
            assert time.monotonic() < deadline
            time.sleep(.01)
        assert len(runtime.sessions.list()) == 0
'''


class NativePublication(unittest.TestCase):
    def test_two_real_extension_instances_survive_version_switch(self):
        with tempfile.TemporaryDirectory(prefix='loaded native 中文 ') as folder:
            state = Path(folder)
            initializer = state / 'init.py'
            initializer.write_text('version = 1\n', encoding='utf-8')
            library = Path(native.__file__)
            suffix = library.name.removeprefix('_native')
            site = state / 'site'
            first = launcher.publish_native(state, site, initializer, library, suffix)
            ready, release = state / 'ready', state / 'release'
            child = subprocess.Popen([sys.executable, '-I', '-c', CHILD, str(site), str(ready), str(release)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 15
                while not ready.exists():
                    self.assertIsNone(child.poll())
                    self.assertLess(time.monotonic(), deadline)
                    time.sleep(.01)
                self.assertEqual(Path(ready.read_text(encoding='utf-8')).parent, first)
                initializer.write_text('version = 2\n', encoding='utf-8')
                second = launcher.publish_native(state, site, initializer, library, suffix)
                self.assertNotEqual(first, second)
                second_ready = state / 'second'
                result = subprocess.run([sys.executable, '-I', '-c', CHILD, str(site), str(second_ready)],
                    capture_output=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
                self.assertEqual(Path(second_ready.read_text(encoding='utf-8')).parent, second)
                self.assertIsNone(child.poll())
                release.touch()
                _, errors = child.communicate(timeout=15)
                self.assertEqual(child.returncode, 0, errors.decode(errors='replace'))
            finally:
                if child.poll() is None:
                    child.kill()
                child.communicate(timeout=10)


if __name__ == '__main__':
    unittest.main()
