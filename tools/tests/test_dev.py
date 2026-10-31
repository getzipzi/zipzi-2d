"""Cross-platform watcher controls using real child processes, without a GUI."""
import os
from contextlib import contextmanager
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'workspace'))
from dev_editor import Editor
from dev_platform import file_lock
from test_dev_lifecycle import FIXTURE


class DevLifecycle(unittest.TestCase):
    external_watcher = False

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='watch 中文 ')
        self.editor = Editor(Path(self.temp.name))
        self.source = self.editor.root / 'Cargo.toml'
        self.source.write_text('first')
        for directory in ('crates', 'vendor', 'tools'):
            (self.editor.root / directory).mkdir()
        self.changed = threading.Event()
        self.watcher = Mock()
        self.watcher.poll.return_value = None
        self.errors = []
        self.thread = None
        self.command = [sys._base_executable, '-c', FIXTURE]
        self.patches = [patch.object(self.editor, 'build', return_value=self.command),
                        patch.object(self.editor, 'publish', side_effect=lambda command: command),
                        patch.object(self.editor, 'guard')]
        if self.external_watcher:
            popen = subprocess.Popen
            def spawn(command, **kwargs):
                if Path(command[0]).stem == 'watchexec':
                    # Exercise a real watchexec event stream on hosts where
                    # native filesystem notifications are unavailable.
                    if os.environ.get('ZIPZI_TEST_WATCHEXEC_NATIVE') != '1':
                        command = [*command, '--poll=100ms']
                    self.watcher = popen(command, **kwargs)
                    return self.watcher
                return popen(command, **kwargs)
            self.patches.append(patch('dev_editor.subprocess.Popen', side_effect=spawn))
        else:
            @contextmanager
            def events():
                yield self.watcher, self.changed
            self.patches.append(patch.object(self.editor, 'watch_events', events))
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
        timeout = 30 if self.external_watcher and os.environ.get('ZIPZI_TEST_WATCHEXEC_NATIVE') == '1' else 10
        deadline = time.monotonic() + timeout
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
        if self.external_watcher:
            # These process fixtures start much faster than a real Cargo build.
            time.sleep(1.)

    def write_source(self, text):
        # notify's polling backend compares whole-second mtimes. Advance the
        # timestamp explicitly so rapid fixture edits are observable there too.
        modified = self.source.stat().st_mtime_ns + 2_000_000_000
        self.source.write_text(text)
        os.utime(self.source, ns=(modified, modified))

    def test_watcher_reloads_then_exits_after_app_closes(self):
        self.start()
        first = self.editor.lifecycle.owners()[0]
        self.write_source('changed source')
        self.changed.set()
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

    def test_edits_during_build_queue_another_reload(self):
        self.start()
        original = self.editor.lifecycle.owners()[0]
        building = threading.Event()
        resume = threading.Event()
        command = self.command
        def build():
            building.set()
            if not resume.wait(5):
                raise RuntimeError('Test did not release the build')
            return command
        self.editor.build.side_effect = build
        self.write_source('first edit')
        self.changed.set()
        self.wait_for(building.is_set)
        try:
            self.write_source('edit during build')
            self.changed.set()
            if self.external_watcher:
                time.sleep(.3)
        finally:
            resume.set()
        self.wait_for(lambda: self.editor.build.call_count >= 3)
        self.wait_for(lambda: any(owner != original for owner in self.editor.lifecycle.owners()))

    def test_failed_build_keeps_host_and_next_edit_retries(self):
        self.start()
        original = self.editor.lifecycle.owners()[0]
        self.editor.build.side_effect = [subprocess.CalledProcessError(17, ['cargo']), self.command]
        self.write_source('broken')
        self.changed.set()
        self.wait_for(lambda: self.editor.build.call_count >= 2)
        self.assertIsNotNone(original.process())
        self.write_source('fixed')
        self.changed.set()
        self.wait_for(lambda: any(owner != original for owner in self.editor.lifecycle.owners()))

    def test_watcher_failure_is_reported_and_preserves_editor(self):
        self.start()
        original = self.editor.lifecycle.owners()[0]
        if self.external_watcher:
            self.watcher.kill()
        else:
            self.watcher.poll.return_value = 17
            self.watcher.returncode = 17
            self.changed.set()
        deadline = time.monotonic() + 5
        while not self.errors and time.monotonic() < deadline:
            time.sleep(.025)
        self.assertEqual(len(self.errors), 1)
        self.assertRegex(str(self.errors[0]), 'watchexec exited')
        self.thread.join(5)
        self.assertFalse(self.thread.is_alive())
        self.assertIsNotNone(original.process())
        if self.external_watcher:
            self.assertIsNotNone(self.watcher.returncode)


@unittest.skipUnless(shutil.which('watchexec'), 'Event-stream integration tests require watchexec')
class WatchexecWatcher(DevLifecycle):
    external_watcher = True

    def test_new_files_atomic_saves_and_ignored_outputs(self):
        (self.editor.root / 'target/ignored.rs').write_text('old build output')
        (self.editor.root / 'crates/ignored.txt').write_text('not source')
        self.start()
        for path in [self.editor.root / 'crates/new.rs', self.source, self.source]:
            first = self.editor.lifecycle.owners()[0]
            temporary = path.with_suffix('.tmp')
            temporary.write_text(str(time.monotonic_ns()))
            modified = (path.stat().st_mtime_ns if path.exists() else time.time_ns()) + 2_000_000_000
            os.utime(temporary, ns=(modified, modified))
            temporary.replace(path)
            self.wait_for(lambda: any(owner != first for owner in self.editor.lifecycle.owners()))
        # Off-scope build output and unrelated extensions must not reload.
        time.sleep(.3)
        builds = self.editor.build.call_count
        (self.editor.root / 'target/ignored.rs').write_text('new build output')
        (self.editor.root / 'crates/ignored.txt').write_text('still not source')
        time.sleep(.5)
        self.assertEqual(self.editor.build.call_count, builds)
        self.stop_when_idle()
        self.thread.join(10)
        self.assertEqual(self.errors, [])


class WatcherDependency(unittest.TestCase):
    def test_missing_watchexec_fails_before_build(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            with patch('dev_editor.shutil.which', return_value=None), patch.object(editor, 'build') as build:
                with self.assertRaisesRegex(RuntimeError, 'cargo install --locked watchexec-cli'):
                    editor.watch()
                build.assert_not_called()
