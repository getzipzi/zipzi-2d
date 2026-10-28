"""Readiness and resident reuse with real Python subprocess fixtures."""
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
import subprocess
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'workspace'))
from dev_lifecycle import Lifecycle

FIXTURE = '''import os,time,pathlib
pid = pathlib.Path(os.environ['ZIPZI_DEV_PID_FILE'])
ready = pathlib.Path(os.environ['ZIPZI_DEV_READY_FILE'])
if os.environ.get('REUSE'):
    pid.write_text('reused:' + os.environ['REUSE'])
else:
    pid.write_text(str(os.getpid()))
    time.sleep(float(os.environ.get('DELAY', '0')))
    ready.write_text('ready')
    time.sleep(60)
'''


class LifecycleTests(unittest.TestCase):
    def test_interrupted_launcher_is_recovered_without_duplicate_owner(self):
        module_path = str(Path(__file__).resolve().parents[1] / 'workspace')
        code = (f'import sys; sys.path.insert(0, {module_path!r}); '
                'from dev_lifecycle import Lifecycle; import os; '
                f'Lifecycle({str(self.lifecycle.directory)!r}).reload('
                f'lambda: [{sys._base_executable!r}, "-c", {FIXTURE!r}], '
                f'cwd={str(self.root)!r}, env=dict(os.environ, DELAY="20"), timeout=30)')
        launcher = subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while not self.lifecycle.owners():
                if launcher.poll() is not None:
                    self.fail(launcher.communicate()[1].decode(errors='replace'))
                if time.monotonic() > deadline:
                    self.fail('Launcher did not record owner')
                time.sleep(.025)
            first, = self.lifecycle.owners()
            # Kill only this test's launcher; its recorded host must survive.
            launcher.kill()
            launcher.communicate(timeout=10)
            self.assertIsNotNone(first.process())
            self.reload()
            self.assertIsNone(first.process())
            self.assertEqual(len(self.lifecycle.owners()), 1)
        finally:
            if launcher.poll() is None:
                launcher.kill()
            launcher.communicate(timeout=10)

    def test_stop_failure_prevents_new_launch_and_preserves_record(self):
        first = self.reload()['owned']
        with patch('dev_lifecycle.stop', side_effect=RuntimeError('did not exit')), patch('dev_lifecycle.launch') as launch:
            with self.assertRaisesRegex(RuntimeError, 'did not exit'):
                self.reload()
            launch.assert_not_called()
        self.assertEqual(self.lifecycle.owners(), [first])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='开发 lifecycle ')
        self.root = Path(self.temp.name)
        self.lifecycle = Lifecycle(self.root / 'state')

    def tearDown(self):
        self.lifecycle.stop()
        self.temp.cleanup()

    def reload(self, **env):
        return self.lifecycle.reload(lambda: [sys._base_executable, '-c', FIXTURE],
            cwd=self.root, env=dict(os.environ, **env), timeout=.4)

    def test_reload_stops_previous_and_failed_build_preserves_it(self):
        first = self.reload()['owned']
        def fail():
            raise RuntimeError('build failed')
        with self.assertRaisesRegex(RuntimeError, 'build failed'):
            self.lifecycle.reload(fail, cwd=self.root, env=os.environ)
        self.assertIsNotNone(first.process())
        second = self.reload()['owned']
        self.assertIsNone(first.process())
        self.assertIsNotNone(second.process())

    def test_timeout_record_recovers_on_next_reload(self):
        with self.assertRaises(TimeoutError):
            self.reload(DELAY='2')
        first, = self.lifecycle.owners()
        self.assertIsNotNone(first.process())
        second = self.reload()['owned']
        self.assertIsNone(first.process())
        self.assertIsNotNone(second.process())

    def test_resident_reuse_does_not_claim_or_stop_resident(self):
        result = self.reload(REUSE=str(os.getpid()))
        self.assertEqual(result, {'reused': os.getpid()})
        self.assertEqual(list(self.lifecycle.records()), [])
        self.lifecycle.stop()

    def test_checkout_isolation(self):
        first = self.reload()['owned']
        other = Lifecycle(self.root / 'other')
        try:
            other.reload(lambda: [sys._base_executable, '-c', FIXTURE], cwd=self.root, env=os.environ)
            other.stop()
            self.assertIsNotNone(first.process())
        finally:
            other.stop()
