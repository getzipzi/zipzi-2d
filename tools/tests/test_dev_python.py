"""Source SDK launcher behavior with real Python processes and stub build tools."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(sys.version_info[:2] == (3, 14), "CPython 3.14 launcher")
class DevPython(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dev python ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "checkout with spaces"
        self.outside = Path(self.temp.name).resolve() / "caller with spaces"
        self.outside.mkdir()
        for name in ("dev-python", "python/dev_python.py", "workspace/dev_platform.py"):
            target = self.root / "tools" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "tools" / name, target)
        self.package = self.root / "crates/zipzi-python2"
        self.source = self.package / "client/python/zipzi/__init__.py"
        self.source.parent.mkdir(parents=True)
        self.source.write_text('value = "initial"\n')
        native = self.package / "native/zipzi_native/__init__.py"
        native.parent.mkdir(parents=True)
        native.write_text('"""Native source fixture."""\n')
        (self.root / "native-source").write_bytes(b"native initial")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.tool("uv", '''import os,pathlib,subprocess,sys
args=sys.argv[1:]
if args[0]=='run':
    os.execv(sys.executable,[sys.executable,*args[args.index('python')+1:]])
assert args[:2]==['pip','install'],args
python=args[args.index('--python')+1]
client=pathlib.Path(args[args.index('--editable')+1])
site=subprocess.check_output([python,'-I','-c',"import sysconfig; print(sysconfig.get_path('purelib'))"],text=True).strip()
(pathlib.Path(site)/'zipzi-source.pth').write_text(str(client/'python')+'\\n')
print('editable prepared',file=sys.stderr)
''')
        self.tool("cargo", '''import json,os,pathlib,sys
root=pathlib.Path.cwd()
args=sys.argv[1:]
assert args[:2]==['rustc','--locked'],args
assert '--features' not in args
assert os.path.isabs(os.environ['PYO3_PYTHON'])
with (root/'build-calls').open('a') as log: log.write('build\\n')
print('native build check',file=sys.stderr)
if os.environ.get('FAIL_BUILD'): sys.exit(17)
target=pathlib.Path(args[args.index('--target-dir')+1])/'debug/libzipzi_python2_native.so'
target.parent.mkdir(parents=True,exist_ok=True)
target.write_bytes((root/'native-source').read_bytes())
print(json.dumps({'reason':'compiler-artifact','target':{'name':'zipzi_python2_native'},'filenames':[str(target)]}))
''')
        self.env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}")
        self.entry = [sys.executable, '-I', str(self.root / 'tools/python/dev_python.py')]

    def tool(self, name, source):
        if os.name == 'nt':
            path = self.bin / (name + '.py')
            path.write_text(source)
            (self.bin / (name + '.cmd')).write_text(f'@"{sys.executable}" -I "{path}" %*\n')
            return
        path = self.bin / name
        path.write_text(f"#!{sys.executable} -I\n" + source)
        path.chmod(0o755)

    def launch(self, *args, input=None, **environment):
        return subprocess.run([*self.entry, *args], cwd=self.outside,
                              env={**self.env, **environment}, input=input,
                              capture_output=True, text=True)

    def assert_ok(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_preserves_cwd_argv_stdin_and_stdout(self):
        script = self.outside / "probe script.py"
        script.write_text("import json,os,sys,zipzi; print(json.dumps([os.getcwd(),sys.argv[1:],sys.stdin.read(),zipzi.value]))")
        result = self.launch("probe script.py", "arg with spaces", "--flag", input="caller input")
        self.assert_ok(result)
        self.assertEqual(json.loads(result.stdout), [str(self.outside), ["arg with spaces", "--flag"], "caller input", "initial"])
        self.assertIn("native build check", result.stderr)

    def test_stdin_code_and_command_code_keep_python_semantics(self):
        result = self.launch("-", "one", input="import sys,zipzi; print(zipzi.value, repr(sys.argv))\n")
        self.assert_ok(result)
        self.assertEqual(result.stdout, "initial ['-', 'one']\n")
        result = self.launch("-I", "-c", "import zipzi; print(zipzi.value)")
        self.assert_ok(result)
        self.assertEqual(result.stdout, "initial\n")
        # With no arguments Python reads a pipe, or opens a REPL on a terminal.
        result = self.launch(input="import zipzi; print(zipzi.value)\n")
        self.assert_ok(result)
        self.assertEqual(result.stdout, "initial\n")

    def test_module_execution_and_user_exit_status(self):
        (self.source.parent / "probe.py").write_text("import sys; print(sys.argv[1]); sys.exit(23)")
        result = self.launch("-m", "zipzi.probe", "argument")
        self.assertEqual(result.returncode, 23, result.stderr)
        self.assertEqual(result.stdout, "argument\n")

    def test_each_launch_reads_updated_python_and_native_sources(self):
        code = ("import pathlib,zipzi,zipzi_native; print(zipzi.value); "
                "print(next(pathlib.Path(zipzi_native.__file__).parent.glob('_native*')).read_text())")
        first = self.launch("-c", code)
        self.assert_ok(first)
        self.assertEqual(first.stdout, "initial\nnative initial\n")
        self.source.write_text('value = "updated source"\n')
        (self.root / "native-source").write_bytes(b"native updated")
        second = self.launch("-c", code)
        self.assert_ok(second)
        self.assertEqual(second.stdout, "updated source\nnative updated\n")
        self.assertEqual((self.root / "build-calls").read_text(), "build\nbuild\n")

    def test_failed_build_never_runs_code_with_previous_artifact(self):
        self.assert_ok(self.launch("-c", "import zipzi"))
        result = self.launch("-c", "print('must not execute')", FAIL_BUILD="1")
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn("preparation failed", result.stderr)

    def test_caller_python_environment_is_cleared_but_host_selection_is_preserved(self):
        shadow = self.outside / "shadow"
        shadow.mkdir()
        (shadow / "zipzi.py").write_text("raise RuntimeError('wrong SDK')")
        result = self.launch("-c", "import os,sys,zipzi; print(zipzi.value); print(os.environ['ZIPZI_RUNTIME_PROFILE']); print(os.environ['VIRTUAL_ENV']==sys.prefix)",
                             PYTHONPATH=str(shadow), PYTHONHOME=str(shadow), ZIPZI_RUNTIME_PROFILE="human-test")
        self.assert_ok(result)
        self.assertEqual(result.stdout, "initial\nhuman-test\nTrue\n")

    def test_concurrent_first_launches_share_preparation_safely(self):
        commands = [[*self.entry, "-c", "import zipzi; print(zipzi.value)"]] * 2
        processes = [subprocess.Popen(command, cwd=self.outside, env=self.env, stdin=subprocess.DEVNULL,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for command in commands]
        try:
            for process in processes:
                stdout, stderr = process.communicate(timeout=30)
                self.assertEqual(process.returncode, 0, stderr)
                self.assertEqual(stdout, "initial\n")
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                process.wait()


class VersionedPublication(unittest.TestCase):
    def test_old_version_survives_and_new_process_resolves_new_package(self):
        spec = importlib.util.spec_from_file_location('dev_python_version', ROOT / 'tools/python/dev_python.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory(prefix='native 中文 ') as folder:
            state = Path(folder)
            init, library = state / 'init.py', state / 'input.dll'
            init.write_text('value = 1\n')
            library.write_bytes(b'old version')
            site = state / 'site'
            first = module.publish_native(state, site, init, library, '.pyd')
            with (first / '_native.pyd').open('rb') as old:
                library.write_bytes(b'new version')
                second = module.publish_native(state, site, init, library, '.pyd')
                self.assertNotEqual(first, second)
                self.assertEqual(old.read(), b'old version')
                self.assertEqual((second / '_native.pyd').read_bytes(), b'new version')
                probe = subprocess.run([sys.executable, '-I', '-c',
                    'import site,sys; site.addsitedir(sys.argv[1]); import zipzi_native; print(zipzi_native.__file__)',
                    str(site)], capture_output=True, text=True, check=True)
                self.assertEqual(Path(probe.stdout.strip()).parent, second)
                self.assertEqual(module.publish_native(state, site, init, library, '.pyd'), second)


@unittest.skipUnless(os.name == "posix", "POSIX extension publication")
class NativePublication(unittest.TestCase):
    def test_replacement_keeps_previously_open_inode_and_skips_unchanged_content(self):
        spec = importlib.util.spec_from_file_location("dev_python_launcher", ROOT / "tools/python/dev_python.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as folder:
            source, destination = Path(folder) / "source", Path(folder) / "installed.so"
            source.write_bytes(b"old")
            module.publish(source, destination)
            inode = destination.stat().st_ino
            module.publish(source, destination)
            self.assertEqual(destination.stat().st_ino, inode)
            with destination.open("rb") as loaded:
                source.write_bytes(b"new")
                module.publish(source, destination)
                self.assertEqual(loaded.read(), b"old")
                self.assertEqual(destination.read_bytes(), b"new")
                self.assertNotEqual(destination.stat().st_ino, inode)


if __name__ == "__main__":
    unittest.main()
