"""Real native process controls, without shell scripts or executable shebangs."""
from dataclasses import replace
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'workspace'))
from dev_process import Identity, launch, stop


class Processes(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='进程 fixture ')
        self.root = Path(self.temp.name)
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=10)
        self.temp.cleanup()

    def start(self, code):
        child = launch([sys.executable, '-c', code], cwd=self.root, env=os.environ.copy(), log=self.root / 'run.log')
        self.children.append(child)
        return child

    def test_wrong_creation_time_or_executable_does_not_stop_process(self):
        child = self.start('import time; time.sleep(60)')
        identity = Identity.capture(child.pid)
        for wrong in (replace(identity, created=identity.created - 1), replace(identity, executable='other')):
            stop(wrong)
            self.assertIsNone(child.poll())
        stop(identity)
        child.wait(timeout=10)

    def test_native_process_tree_exits_and_other_owner_survives(self):
        unrelated = self.start('import time; time.sleep(60)')
        child = self.start("import subprocess,sys,time,pathlib; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); pathlib.Path('child.pid').write_text(str(p.pid)); time.sleep(60)")
        deadline = time.monotonic() + 10
        pid_file = self.root / 'child.pid'
        while not pid_file.exists():
            if time.monotonic() >= deadline:
                self.fail('Child did not publish pid')
            time.sleep(.02)
        descendant = Identity.capture(int(pid_file.read_text()))
        owner = Identity.capture(child.pid)
        try:
            stop(owner)
            child.wait(timeout=10)
            self.assertIsNone(descendant.process())
            self.assertIsNone(unrelated.poll())
        finally:
            stop(descendant)

    def test_log_cwd_and_exit_status(self):
        child = self.start("import pathlib; print(pathlib.Path.cwd().name, flush=True); raise SystemExit(23)")
        self.assertEqual(child.wait(timeout=10), 23)
        self.assertIn(self.root.name, (self.root / 'run.log').read_text(encoding='utf-8'))
