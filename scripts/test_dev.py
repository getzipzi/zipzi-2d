"""Dev lifecycle regression tests. Uses fake tools; never starts a GUI app."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPT = Path(__file__).with_name("dev.sh")


class DevLifecycle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="kasane-dev-test-")
        self.root = Path(self.temp.name) / "editor"
        (self.root / "scripts").mkdir(parents=True)
        (self.root.parent / "kasane-2d/modules").mkdir(parents=True)
        shutil.copy(SCRIPT, self.root / "scripts/dev.sh")
        (self.root / "target/debug").mkdir(parents=True)
        # Copying an Apple platform-signed /bin/sleep is killed by macOS code
        # signing. Build a tiny, owned stand-in with normal process semantics.
        subprocess.run(["cc", "-x", "c", "-", "-o", str(self.root / "target/debug/kasane-editor")],
                       input='#include <unistd.h>\n#include <stdlib.h>\n#include <signal.h>\nint main(void) { if (getenv("IGNORE_TERM")) signal(SIGTERM, SIG_IGN); for (;;) pause(); }',
                       text=True, capture_output=True, check=True)
        self.bin = self.root / "tools"
        self.bin.mkdir()
        self.tool("cargo", """
import os,time,pathlib
pathlib.Path('build-started').touch()
while pathlib.Path('hold-build').exists(): time.sleep(0.02)
raise SystemExit(int(os.environ.get('BUILD_FAIL','0')))
""")
        self.tool("open", """
import os,pathlib,subprocess,sys,time
args=sys.argv[1:]
pid_file=next(a.split('=',1)[1] for a in args if a.startswith('KASANE_DEV_PID_FILE='))
with open('launches','a') as f: f.write(pid_file+'\\n')
if os.environ.get('OPEN_FAIL'): raise SystemExit(1)
exe=pathlib.Path(args[-1])/'Contents/MacOS/kasane-editor'
p=subprocess.Popen([str(exe),'600'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
with open('test-pids','a') as f: f.write(str(p.pid)+'\\n')
time.sleep(float(os.environ.get('READY_DELAY','0')))
if not os.environ.get('NO_READY'): pathlib.Path(pid_file).write_text(str(p.pid))
""")
        self.tool("watchexec", """
import pathlib,subprocess,time
while True:
    trigger=pathlib.Path('trigger-reload')
    if trigger.exists():
        trigger.unlink()
        subprocess.run(['sh','scripts/dev.sh','editor','reload'],check=False)
    time.sleep(0.05)
""")
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}"}
        self.pid_file = self.root / "target/dev-kasane-editor.pid"
        self.children = []

    def tool(self, name, body):
        path = self.bin / name
        path.write_text(f"#!{sys.executable}\n{body}")
        path.chmod(0o755)

    def start(self, mode="reload", **env):
        p = subprocess.Popen(["sh", "scripts/dev.sh", "editor", mode],cwd=self.root,
                             env={**self.env, **env},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        self.children.append(p)
        return p

    def result(self, p):
        output = p.communicate(timeout=15)[0]
        return p.returncode, output

    def run_reload(self, **env):
        return self.result(self.start(**env))

    def wait_for(self, condition):
        deadline = time.monotonic()+5
        while not condition():
            if time.monotonic()>deadline: self.fail("Timed out")
            time.sleep(0.02)

    def alive(self, pid):
        r=subprocess.run(["ps","-p",str(pid),"-o","stat="],capture_output=True,text=True)
        return r.returncode==0 and not r.stdout.strip().startswith("Z")

    def tearDown(self):
        (self.root / "hold-build").unlink(missing_ok=True)
        for p in self.children:
            if p.poll() is None: p.terminate()
            p.communicate(timeout=10)
        recorded = self.root / "test-pids"
        if recorded.exists():
            for pid in recorded.read_text().splitlines():
                try: os.kill(int(pid), 9)
                except ProcessLookupError: pass
        self.temp.cleanup()

    def test_sequential_reload_stops_previous_and_publishes_private_readiness(self):
        self.assertEqual(self.run_reload()[0],0)
        first=int(self.pid_file.read_text())
        self.assertTrue(self.alive(first))
        self.assertEqual(self.run_reload()[0],0)
        second=int(self.pid_file.read_text())
        self.assertNotEqual(first,second)
        self.assertFalse(self.alive(first))
        self.assertTrue(self.alive(second))
        ready=(self.root/'launches').read_text().splitlines()
        self.assertEqual(len(set(ready)),2)
        self.assertTrue(all('.launch.' in p for p in ready))

    def test_failed_build_keeps_existing_app(self):
        self.assertEqual(self.run_reload()[0],0)
        pid=int(self.pid_file.read_text())
        self.assertNotEqual(self.run_reload(BUILD_FAIL='1')[0],0)
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_concurrent_reload_is_rejected(self):
        (self.root/'hold-build').touch()
        first=self.start()
        self.wait_for(lambda:(self.root/'build-started').exists())
        code,output=self.run_reload()
        self.assertNotEqual(code,0)
        self.assertIn('already running',output)
        self.assertFalse((self.root/'launches').exists())
        (self.root/'hold-build').unlink()
        self.assertEqual(self.result(first)[0],0)

    def test_second_watcher_is_rejected(self):
        self.start('watch')
        self.wait_for(lambda:(self.root/'target/dev-kasane-editor.watch.lock/owner').exists())
        code,output=self.result(self.start('watch'))
        self.assertNotEqual(code,0)
        self.assertIn('already running',output)

    def test_watcher_survives_restart_gap_but_stops_when_app_is_closed(self):
        self.assertEqual(self.run_reload()[0],0)
        first=int(self.pid_file.read_text())
        watcher=self.start('watch',READY_DELAY='1.2')
        self.wait_for(lambda:(self.root/'target/dev-kasane-editor.watch.lock/owner').exists())
        (self.root/'trigger-reload').touch()
        self.wait_for(lambda:int(self.pid_file.read_text()) != first)
        self.assertIsNone(watcher.poll())
        self.assertFalse(self.alive(first))
        os.kill(int(self.pid_file.read_text()),15)
        code,output=self.result(watcher)
        self.assertEqual(code,0,output)
        self.assertIn('stopping the file watcher',output)
        self.assertFalse((self.root/'target/dev-kasane-editor.watch.lock').exists())

    def test_lost_pid_does_not_spawn_or_kill_untracked_instance(self):
        self.assertEqual(self.run_reload()[0],0)
        pid=int(self.pid_file.read_text())
        self.pid_file.unlink()
        code,output=self.run_reload()
        self.assertNotEqual(code,0)
        self.assertIn('Untracked',output)
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_old_instance_refusing_term_blocks_new_launch(self):
        self.assertEqual(self.run_reload(IGNORE_TERM='1')[0],0)
        pid=int(self.pid_file.read_text())
        code,output=self.run_reload()
        self.assertNotEqual(code,0)
        self.assertIn('did not exit',output)
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_readiness_timeout_does_not_accumulate_instances(self):
        self.assertNotEqual(self.run_reload(NO_READY='1')[0],0)
        code,output=self.run_reload()
        self.assertNotEqual(code,0)
        self.assertIn('Untracked',output)
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)


if __name__ == '__main__':
    unittest.main()
