"""Cross-platform watcher controls using real child processes, without a GUI."""
import os
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'workspace'))
from dev_editor import Editor
from dev_platform import file_lock
from test_dev_lifecycle import FIXTURE


class DevLifecycle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='watch 中文 ')
        self.editor = Editor(Path(self.temp.name))
        self.source = self.editor.root / 'Cargo.toml'
        self.source.write_text('first')
        self.errors = []
        self.thread = None
        self.command = [sys._base_executable, '-c', FIXTURE]
        self.patches = [patch.object(self.editor, 'build', return_value=self.command),
                        patch.object(self.editor, 'publish', side_effect=lambda command: command),
                        patch.object(self.editor, 'guard')]
        for item in self.patches:
            item.start()

    def tearDown(self):
        self.stop_when_idle()
        if self.thread:
            self.thread.join(10)
            self.assertFalse(self.thread.is_alive())
        for item in self.patches:
            item.stop()
        self.temp.cleanup()

    def stop_when_idle(self):
        deadline = time.monotonic() + 10
        while True:
            try:
                self.editor.lifecycle.stop()
                return
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(.025)

    def wait_for(self, condition):
        deadline = time.monotonic() + 10
        while not condition():
            if self.errors:
                raise self.errors[0]
            if time.monotonic() > deadline:
                self.fail('Watcher did not reach expected state')
            time.sleep(.025)

    def start(self):
        def watch():
            try:
                self.editor.watch()
            except BaseException as error:
                self.errors.append(error)
        self.thread = threading.Thread(target=watch)
        self.thread.start()
        self.wait_for(lambda: bool(self.editor.lifecycle.owners()))

    def test_watcher_reloads_then_exits_after_app_closes(self):
        self.start()
        first = self.editor.lifecycle.owners()[0]
        self.source.write_text('changed source')
        self.wait_for(lambda: any(owner != first for owner in self.editor.lifecycle.owners()))
        self.assertIsNone(first.process())
        self.stop_when_idle()
        self.thread.join(10)
        self.assertFalse(self.thread.is_alive())
        self.assertEqual(self.errors, [])

    def test_second_watcher_is_rejected(self):
        self.start()
        with self.assertRaises(BlockingIOError):
            self.editor.watch()

    def test_watcher_survives_external_reload_gap(self):
        self.start()
        # Serialize with initial startup before simulating another launcher's gap.
        with file_lock(self.editor.state / 'operation.lock'):
            self.editor.lifecycle._stop()
            time.sleep(.8)
            self.assertTrue(self.thread.is_alive())
        self.editor.reload()
        self.assertTrue(self.thread.is_alive())

    def test_concurrent_reload_is_rejected_before_build(self):
        with file_lock(self.editor.state / 'operation.lock', blocking=False):
            with self.assertRaises(BlockingIOError):
                self.editor.reload()
        self.editor.build.assert_not_called()

    def test_pid_without_readiness_does_not_succeed(self):
        code = "import os,time,pathlib; pathlib.Path(os.environ['ZIPZI_DEV_PID_FILE']).write_text(str(os.getpid())); time.sleep(60)"
        with self.assertRaises(TimeoutError):
            self.editor.lifecycle.reload(lambda: [sys._base_executable, '-c', code],
                cwd=self.editor.root, env=os.environ, timeout=.4)
        self.assertEqual(len(self.editor.lifecycle.owners()), 1)

    def test_watch_reused_host_exits_without_ownership(self):
        self.editor.env['REUSE'] = str(os.getpid())
        self.assertEqual(self.editor.watch(), 0)
        self.assertEqual(self.editor.lifecycle.owners(), [])
