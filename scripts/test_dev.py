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
    @classmethod
    def setUpClass(cls):
        cls._shared_temp = tempfile.TemporaryDirectory(prefix="kasane-dev-stub-")
        cls._stub_binary = Path(cls._shared_temp.name) / "kasane-editor"
        subprocess.run(["cc", "-x", "c", "-", "-o", str(cls._stub_binary)],
                       input='''#include <unistd.h>
#include <stdlib.h>
#include <signal.h>
#include <stdio.h>
int main(void) {
    if (getenv("IGNORE_TERM")) signal(SIGTERM, SIG_IGN);
    const char *delay = getenv("READY_DELAY");
    if (delay) usleep((useconds_t)(atof(delay) * 1000000));
    const char *path = getenv("KASANE_DEV_PID_FILE");
    if (path && !getenv("NO_READY")) {
        FILE *file = fopen(path, "w");
        if (file) { fprintf(file, "%d", getpid()); fclose(file); }
    }
    for (;;) pause();
}''',
                       text=True, capture_output=True, check=True)

    @classmethod
    def tearDownClass(cls):
        cls._shared_temp.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="kasane-dev-test-")
        self.root = Path(self.temp.name) / "editor"
        (self.root / "scripts").mkdir(parents=True)
        (self.root.parent / "kasane-2d/modules").mkdir(parents=True)
        shutil.copy(SCRIPT, self.root / "scripts/dev.sh")
        (self.root / "LICENSE").write_text("Dev lifecycle fixture license\n")
        (self.root / "target/debug").mkdir(parents=True)
        shutil.copy(self._stub_binary, self.root / "target/debug/kasane-editor")
        self.bin = self.root / "tools"
        self.bin.mkdir()
        self.tool("cargo", """
import os,time,pathlib
pathlib.Path('build-started').touch()
while pathlib.Path('hold-build').exists(): time.sleep(0.02)
raise SystemExit(int(os.environ.get('BUILD_FAIL','0')))
""")
        self.tool("open", """
import os,pathlib,subprocess,sys
args=sys.argv[1:]
pid_file=next(a.split('=',1)[1] for a in args if a.startswith('KASANE_DEV_PID_FILE='))
with open('launches','a') as f: f.write(pid_file+'\\n')
if os.environ.get('OPEN_FAIL'): raise SystemExit(1)
exe=pathlib.Path(args[-1])/'Contents/MacOS/kasane-editor'
env=dict(os.environ)
for i,arg in enumerate(args):
    if arg == '--env':
        key,value=args[i+1].split('=',1)
        env[key]=value
p=subprocess.Popen([str(exe),'600'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
with open('test-pids','a') as f: f.write(str(p.pid)+'\\n')
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
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "KASANE_DEV_ATTEMPTS": "50",
            "KASANE_DEV_SLEEP_INTERVAL": "0.04",
            "KASANE_DEV_WATCH_INTERVAL": "0.05",
        }
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

    def run_reload(self, *, expected=0, **env):
        code, output = self.result(self.start(**env))
        if expected is not None:
            self.assertEqual(code, expected, output)
        return code, output

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
        self.run_reload()
        first=int(self.pid_file.read_text())
        self.assertTrue(self.alive(first))
        self.run_reload()
        second=int(self.pid_file.read_text())
        self.assertNotEqual(first,second)
        self.assertFalse(self.alive(first))
        self.assertTrue(self.alive(second))
        ready=(self.root/'launches').read_text().splitlines()
        self.assertEqual(len(set(ready)),2)
        self.assertTrue(all('.launch.' in p for p in ready))

    def test_failed_build_keeps_existing_app(self):
        self.run_reload()
        pid=int(self.pid_file.read_text())
        self.run_reload(expected=1, BUILD_FAIL='1')
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_concurrent_reload_is_rejected(self):
        (self.root/'hold-build').touch()
        first=self.start()
        self.wait_for(lambda:(self.root/'build-started').exists())
        code,output=self.run_reload(expected=None)
        self.assertNotEqual(code,0)
        self.assertIn('already running',output)
        self.assertFalse((self.root/'launches').exists())
        (self.root/'hold-build').unlink()
        code,output=self.result(first)
        self.assertEqual(code,0,output)

    def test_second_watcher_is_rejected(self):
        self.start('watch')
        self.wait_for(lambda:(self.root/'target/dev-kasane-editor.watch.lock/owner').exists())
        code,output=self.result(self.start('watch'))
        self.assertNotEqual(code,0)
        self.assertIn('already running',output)

    def test_watcher_survives_restart_gap_but_stops_when_app_is_closed(self):
        self.run_reload()
        first=int(self.pid_file.read_text())
        watcher=self.start('watch',READY_DELAY='0.8')
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
        self.run_reload()
        pid=int(self.pid_file.read_text())
        self.pid_file.unlink()
        code,output=self.run_reload(expected=None)
        self.assertNotEqual(code,0)
        self.assertIn('Untracked',output)
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_old_instance_refusing_term_blocks_new_launch(self):
        self.run_reload(IGNORE_TERM='1')
        pid=int(self.pid_file.read_text())
        code,output=self.run_reload(expected=None)
        self.assertNotEqual(code,0)
        self.assertIn('did not exit',output)
        self.assertTrue(self.alive(pid))
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_readiness_timeout_does_not_accumulate_instances(self):
        self.run_reload(expected=1, NO_READY='1')
        code,output=self.run_reload(expected=None)
        self.assertNotEqual(code,0)
        self.assertIn('Untracked',output)
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)

    def test_late_pid_after_timeout_is_recovered_on_next_reload(self):
        code,output=self.run_reload(expected=None, READY_DELAY='2.5')
        self.assertNotEqual(code,0,output)
        pid=int((self.root/'test-pids').read_text())
        pending=Path((self.root/'launches').read_text().strip())
        self.assertTrue(pending.exists())
        self.wait_for(lambda:pending.exists() and pending.stat().st_size > 0)
        self.assertEqual(int(pending.read_text()),pid)
        code,output=self.run_reload(expected=None)
        self.assertEqual(code,0,output)
        self.assertIn('Recovered',output)
        self.assertFalse(self.alive(pid))
        self.assertFalse(pending.exists())
        self.assertTrue(self.alive(int(self.pid_file.read_text())))

    def test_interrupted_launch_keeps_pid_record_for_recovery(self):
        reload=self.start(READY_DELAY='0.8')
        self.wait_for(lambda:(self.root/'test-pids').exists())
        pid=int((self.root/'test-pids').read_text())
        pending=Path((self.root/'launches').read_text().strip())
        reload.terminate()
        self.assertNotEqual(self.result(reload)[0],0)
        self.assertTrue(pending.exists())
        self.wait_for(lambda:pending.exists() and pending.stat().st_size > 0)
        code,output=self.run_reload(expected=None)
        self.assertEqual(code,0,output)
        self.assertIn('Recovered',output)
        self.assertFalse(self.alive(pid))

    def test_multiple_tracked_instances_are_preserved(self):
        self.run_reload()
        first=int(self.pid_file.read_text())
        pending=self.pid_file.with_name(self.pid_file.name+'.launch.extra')
        exe=(self.root/'target/Kasane Editor Dev.app/Contents/MacOS/kasane-editor').resolve()
        extra=subprocess.Popen([str(exe)],env={**self.env,'KASANE_DEV_PID_FILE':str(pending)},
                               stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        self.children.append(extra)
        self.wait_for(lambda:pending.exists() and pending.stat().st_size > 0)
        code,output=self.run_reload(expected=None)
        self.assertNotEqual(code,0,output)
        self.assertIn('Multiple tracked',output)
        self.assertTrue(self.alive(first))
        self.assertTrue(self.alive(extra.pid))
        self.assertEqual(int(self.pid_file.read_text()),first)
        self.assertEqual(len((self.root/'launches').read_text().splitlines()),1)


if __name__ == '__main__':
    unittest.main()
