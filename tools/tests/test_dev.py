"""Dev lifecycle regression tests. Uses fake tools; never starts a GUI app."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))

SCRIPT = (Path(__file__).resolve().parents[2] / "tools").joinpath("dev-editor.sh")


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
    const char *gate = getenv("READY_GATE");
    while (gate && access(gate, F_OK) == 0) usleep(1000);
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
        self.root = Path(self.temp.name).resolve() / "editor"
        self.app = self.root / "crates/kasane-editor"
        (self.root / "tools").mkdir(parents=True)
        (self.root / "crates/kasane-editor").mkdir(parents=True)
        (self.root / "Cargo.toml").touch()
        (self.root / "Cargo.lock").touch()
        for name in ["dev-editor.sh", "workspace/workspace.sh", "workspace/workspace_tools.py"]:
            (self.root / "tools" / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(SCRIPT.parent / name, self.root / "tools" / name)
        (self.root / "LICENSE-GPL-3.0").write_text("Dev lifecycle fixture license\n")
        self.bin = self.root / "tools"
        self.bin.mkdir(exist_ok=True)
        shutil.copy(self._stub_binary, self.bin / "stub-editor")
        self.tool("cargo", """
import json,os,time,pathlib,shutil,sys
args=sys.argv[1:]
target=pathlib.Path(args[args.index('--target-dir')+1] if '--target-dir' in args else os.environ.get('CARGO_TARGET_DIR','target'))
with open('cargo-invocations.jsonl','a') as f:
    f.write(json.dumps({'args':args,'target_dir':str(target.resolve())})+'\\n')
pathlib.Path('build-started').touch()
while pathlib.Path('hold-build').exists(): time.sleep(0.02)
if os.environ.get('BUILD_FAIL'): raise SystemExit(int(os.environ['BUILD_FAIL']))
folder=target/os.environ['CARGO_BUILD_TARGET'] if os.environ.get('CARGO_BUILD_TARGET') else target
(folder/'debug').mkdir(parents=True,exist_ok=True)
binary=folder/'debug/kasane-editor'
shutil.copy('tools/stub-editor',binary)
print(json.dumps({'reason':'compiler-artifact','target':{'name':'kasane-editor','kind':['bin']},'executable':str(binary.resolve())}))
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
import pathlib,subprocess,time,sys,json
pathlib.Path('watch-args.json').write_text(json.dumps(sys.argv[1:]))
while True:
    trigger=pathlib.Path('trigger-reload')
    if trigger.exists():
        trigger.unlink()
        subprocess.run(['sh','tools/dev-editor.sh','editor','reload'],check=False)
    time.sleep(0.05)
""")
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "CARGO_BUILD_TARGET": "",
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
        p = subprocess.Popen(["sh", "tools/dev-editor.sh", "editor", mode],cwd=self.root,
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

    def test_reload_build_and_run_use_separate_target_from_tests(self):
        inherited_target = self.root / 'test target'
        (inherited_target / 'debug').mkdir(parents=True)
        test_binary = inherited_target / 'debug/kasane-editor'
        test_binary.write_text('test build must not be replaced or launched\n')
        self.run_reload(CARGO_TARGET_DIR=str(inherited_target))
        code, output = self.result(self.start('run', CARGO_TARGET_DIR=str(inherited_target)))
        self.assertEqual(code, 0, output)

        invocations = [json.loads(line) for line in
                       (self.root / 'cargo-invocations.jsonl').read_text().splitlines()]
        self.assertEqual([entry['args'][0] for entry in invocations], ['build', 'run'])
        expected_target = str((self.root / 'target/dev').resolve())
        self.assertTrue(all(entry['target_dir'] == expected_target for entry in invocations))
        self.assertFalse((self.root / 'target/debug').exists())
        self.assertEqual(test_binary.read_text(), 'test build must not be replaced or launched\n')
        app_binary = self.root / 'target/Kasane Editor Dev.app/Contents/MacOS/kasane-editor'
        self.assertEqual(app_binary.read_bytes(), self._stub_binary.read_bytes())

    def test_watcher_reload_uses_separate_target(self):
        watcher = self.start('watch')
        self.wait_for(lambda:(self.root/'target/dev-kasane-editor.watch.lock/owner').exists())
        (self.root / 'trigger-reload').touch()
        self.wait_for(lambda:self.pid_file.exists() and self.pid_file.stat().st_size > 0)
        self.assertTrue((self.root / 'target/dev/debug/kasane-editor').exists())
        self.assertFalse((self.root / 'target/debug').exists())
        os.kill(int(self.pid_file.read_text()), 15)
        code, output = self.result(watcher)
        self.assertEqual(code, 0, output)

    def test_target_triple_and_workspace_watch_paths(self):
        self.run_reload(CARGO_BUILD_TARGET='aarch64-apple-darwin')
        self.assertEqual((self.root/'target/Kasane Editor Dev.app/Contents/MacOS/kasane-editor').read_bytes(), self._stub_binary.read_bytes())
        watcher = self.start('watch')
        self.wait_for(lambda:(self.root/'watch-args.json').exists())
        args = json.loads((self.root/'watch-args.json').read_text())
        for path in [self.root/'crates', self.root/'vendor', self.root/'Cargo.toml', self.root/'Cargo.lock']:
            self.assertIn(str(path), args)
        self.assertIn(str(self.root/'tools/dev-editor.sh'), args)

    def test_absolute_entrypoint_from_app_and_other_cwd_and_checkout_isolation(self):
        for cwd in [self.app, self.root.parent]:
            child = subprocess.Popen(['sh',str(self.root/'tools/dev-editor.sh'),'editor','reload'],cwd=cwd,
                env=self.env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
            self.children.append(child)
            code, output = self.result(child)
            self.assertEqual(code,0,output)
        first = int(self.pid_file.read_text())
        other = self.root.parent/'second checkout'
        shutil.copytree(self.app, other/'crates/kasane-editor')
        shutil.copy(self.root/'LICENSE-GPL-3.0', other/'LICENSE-GPL-3.0')
        shutil.copytree(self.bin, other/'tools')
        for _ in range(2):
            child = subprocess.Popen(['sh',str(other/'tools/dev-editor.sh'),'editor','reload'],cwd=self.root.parent,
                env=self.env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
            self.children.append(child)
            code, output = self.result(child)
            # Include any processes started by the second checkout in teardown.
            recorded = other/'test-pids'
            if recorded.exists():
                with (self.root/'test-pids').open('a') as log:
                    log.write(recorded.read_text())
            self.assertEqual(code,0,output)
            self.assertTrue(self.alive(first))
            self.assertNotEqual(first,int((other/'target/dev-kasane-editor.pid').read_text()))

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
        # Keep readiness blocked until the timeout has actually returned. A
        # fixed delay races the shell's polling/ps overhead on busy machines.
        gate=self.root/'hold-readiness'
        gate.touch()
        code,output=self.run_reload(expected=None, READY_GATE=str(gate))
        self.assertNotEqual(code,0,output)
        pid=int((self.root/'test-pids').read_text())
        pending=Path((self.root/'launches').read_text().strip())
        self.assertTrue(pending.exists())
        gate.unlink()
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
