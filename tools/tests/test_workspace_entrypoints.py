"""Exercise packaging from real directory layouts with a fake Cargo artifact stream."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/workspace"))

from workspace_tools import executable, source_manifest, repository

SCRIPTS = Path(__file__).resolve().parents[2] / "tools"


class PackagingPaths(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='editor paths ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / 'workspace'
        self.app = self.root / 'crates/zipzi-editor'
        (self.root / 'tools').mkdir(parents=True)
        self.app.mkdir(parents=True)
        (self.app / 'assets/app-icons').mkdir(parents=True)
        for name in ['workspace/workspace.sh', 'workspace/workspace_tools.py', 'build-editor-app.sh']:
            (self.root / "tools" / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(SCRIPTS / name, self.root / 'tools' / name)
        (self.root / 'LICENSE-GPL-3.0').write_text('Editor GPL fixture')
        (self.root / 'LICENSE-MIT').write_text('SDK MIT fixture')
        for name in ['LICENSING.md', 'THIRD_PARTY_NOTICES.md']:
            (self.root/name).write_text(name)
        (self.root/'vendor/library').mkdir(parents=True)
        (self.root/'vendor/README.md').write_text('vendor provenance')
        (self.root/'vendor/library/LICENSE').write_text('third-party license')
        (self.root / 'Cargo.toml').write_text('[workspace]')
        (self.root / 'Cargo.lock').write_text('version = 4')
        (self.app / 'assets/app-icons/zipzi-editor-icon-vector.icns').write_bytes(b'icon')
        self.outside = Path(self.temp.name).resolve() / 'elsewhere'
        self.outside.mkdir()
        self.bin = Path(self.temp.name).resolve() / 'bin'
        self.bin.mkdir()
        cargo = self.bin / 'cargo'
        cargo.write_text(f'#!{sys.executable}\n' + '''import json,os,pathlib,sys
args=sys.argv[1:]
if os.environ.get('BUILD_FAIL'): sys.exit(7)
root=pathlib.Path.cwd()
if args[0]=='test':
    output=pathlib.Path(os.environ['ZIPZI_TIMELINE_OUTPUT'])
    for name in ['results.json','number-probe.json','trace.jsonl']: (output/name).write_text('{}')
    sys.exit(0)
profile='release' if args[args.index('--profile')+1]=='release' else 'debug'
target=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target')))
if os.environ.get('CARGO_BUILD_TARGET'): target/=os.environ['CARGO_BUILD_TARGET']
binary=target/profile/'zipzi-editor'
binary.parent.mkdir(parents=True,exist_ok=True)
binary.write_bytes(b'actual artifact')
(root/'invocation.json').write_text(json.dumps({'cwd':str(root),'target':str(target),'args':args}))
print(json.dumps({'reason':'compiler-artifact','target':{'name':'zipzi-editor','kind':['bin']},'executable':str(binary)}))
''')
        cargo.chmod(0o755)
        self.env = {**os.environ, 'PATH': f'{self.bin}:{os.environ["PATH"]}', 'CARGO_BUILD_TARGET': 'aarch64-apple-darwin'}

    def test_bundle_uses_artifact_and_editor_license_from_each_cwd(self):
        for cwd in [self.root, self.app, self.outside]:
            for profile in ['debug', 'release']:
                with self.subTest(cwd=cwd, profile=profile):
                    result = subprocess.run(['sh', str(self.root/'tools/build-editor-app.sh'), 'editor', profile], cwd=cwd,
                        env={**self.env, 'CARGO_TARGET_DIR': 'custom target'}, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    bundle = Path(result.stdout.strip())
                    self.assertEqual(bundle, self.root/'target/Zipzi Editor.app')
                    self.assertEqual((bundle/'Contents/MacOS/zipzi-editor').read_bytes(), b'actual artifact')
                    self.assertEqual((bundle/'Contents/Resources/LICENSE').read_text(), 'Editor GPL fixture')
                    self.assertEqual((bundle/'Contents/Resources/AppIcon.icns').read_bytes(), b'icon')
                    self.assertEqual((bundle/'Contents/Resources/LICENSE-MIT').read_text(), 'SDK MIT fixture')
                    self.assertEqual((bundle/'Contents/Resources/THIRD_PARTY_NOTICES.md').read_text(), 'THIRD_PARTY_NOTICES.md')
                    self.assertEqual((bundle/'Contents/Resources/vendor-notices/library/LICENSE').read_text(), 'third-party license')
                    call = json.loads((self.root/'invocation.json').read_text())
                    self.assertEqual(Path(call['target']), cwd/'custom target/aarch64-apple-darwin')
                    before = (bundle/'Contents/MacOS/zipzi-editor').read_bytes()
                    failed = subprocess.run(['sh', str(self.root/'tools/build-editor-app.sh'), 'editor', profile], cwd=cwd,
                        env={**self.env, 'BUILD_FAIL': '1'}, capture_output=True, text=True)
                    self.assertEqual(failed.returncode, 7)
                    self.assertEqual((bundle/'Contents/MacOS/zipzi-editor').read_bytes(), before)

    def test_benchmark_wrappers_preserve_caller_paths_and_single_repository_provenance(self):
        for name in ['window-benchmark.sh', 'properties-benchmark.sh', 'physics-properties-benchmark.sh', 'timeline-benchmark.sh', 'timeline-number-benchmark.sh']:
            (self.root/'tools/benchmarks').mkdir(exist_ok=True)
            shutil.copy(SCRIPTS/'benchmarks'/name, self.root/'tools/benchmarks'/name)
        for name in ['frame-trace-report.py', 'properties-trace-report.py', 'animation-trace-report.py']:
            (self.root/'tools/benchmarks'/name).write_text('print("{}")')
        for name in ['sw_vers','sysctl','system_profiler','ioreg','caffeinate']:
            tool = self.bin/name
            tool.write_text(f'#!{sys.executable}\n' + """import json,os,pathlib,sys
if pathlib.Path(sys.argv[0]).name=='caffeinate':
    pathlib.Path(os.environ['ZIPZI_FRAME_TRACE']).write_text('{}')
    pathlib.Path('launch.json').write_text(json.dumps({'binary':sys.argv[-1], 'model':os.environ['ZIPZI_BENCH_MODEL'], 'suite':os.environ.get('ZIPZI_BENCH_PROPERTY_SUITE'), 'warmup':os.environ.get('ZIPZI_BENCH_WARMUP'), 'samples':os.environ.get('ZIPZI_BENCH_SAMPLES')}))
""")
            tool.chmod(0o755)
        subprocess.run(['git','init','-q'],cwd=self.root,check=True)
        subprocess.run(['git','add','.'],cwd=self.root,check=True)
        subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.test','commit','-qm','fixture'],cwd=self.root,check=True)
        counter = 0
        for cwd in [self.root, self.app, self.outside]:
            model = cwd/'model space/model.json'
            model.parent.mkdir(exist_ok=True)
            model.write_text('{}')
            for script in ['window-benchmark.sh','properties-benchmark.sh','physics-properties-benchmark.sh','timeline-benchmark.sh','timeline-number-benchmark.sh']:
                counter += 1
                output = f'output {counter}'
                args = ['model space/model.json',output,'2','120'] if script=='window-benchmark.sh' else [output,'2']
                result = subprocess.run(['bash',str(self.root/'tools/benchmarks'/script),*args],cwd=cwd,
                    env={**self.env,'CARGO_TARGET_DIR':'custom target','ZIPZI_BENCH_MODEL':'model space/model.json',
                         'ZIPZI_TIMELINE_MODEL':'model space/model.json'},capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stderr)
                provenance = json.loads((cwd/output/'provenance.json').read_text())
                self.assertEqual(provenance['model'],str(model))
                self.assertIn('repository',provenance)
                self.assertNotIn('repositories',provenance)
                self.assertEqual(provenance['build_environment']['CARGO_TARGET_DIR'],str(cwd/'custom target'))
                if script in ['window-benchmark.sh','properties-benchmark.sh','physics-properties-benchmark.sh']:
                    launch = json.loads((self.root/'launch.json').read_text())
                    self.assertEqual(launch['model'],str(model))
                    self.assertEqual(Path(launch['binary']),cwd/'custom target/aarch64-apple-darwin/release/zipzi-editor')
                    if script=='physics-properties-benchmark.sh':
                        self.assertEqual(launch['suite'],'physics')
                        self.assertEqual(launch['samples'],'2')
                        self.assertEqual(launch['warmup'],'5')
        failed = subprocess.run(['bash',str(self.root/'tools/benchmarks/timeline-benchmark.sh'),'failure'],cwd=self.outside,
            env={**self.env,'ZIPZI_TIMELINE_MODEL':str(model),'BUILD_FAIL':'1'},capture_output=True,text=True)
        self.assertEqual(failed.returncode,7,failed.stderr)
        missing = subprocess.run(['bash',str(self.root/'tools/benchmarks/window-benchmark.sh'),'missing-model.json','missing-output'],cwd=self.outside,
            env=self.env,capture_output=True,text=True)
        self.assertNotEqual(missing.returncode,0)
        self.assertIn('missing-model.json',missing.stderr)

    def test_physics_wrapper_rejects_unrestorable_history_before_building(self):
        script = self.root/'tools/benchmarks/physics-properties-benchmark.sh'
        script.parent.mkdir(exist_ok=True)
        shutil.copy(SCRIPTS/'benchmarks/physics-properties-benchmark.sh', script)
        for samples, warmup in [('46', '5'), ('40', '11'), ('invalid', '5'), ('2', 'invalid')]:
            with self.subTest(samples=samples, warmup=warmup):
                result = subprocess.run(['bash', str(script), 'output space', samples],
                    cwd=self.outside, env={**self.env, 'ZIPZI_BENCH_WARMUP': warmup},
                    capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn('Physics', result.stderr)
                self.assertFalse((self.root/'invocation.json').exists())

    def test_ambiguous_or_missing_artifacts_fail(self):
        item = {'reason':'compiler-artifact', 'target':{'name':'zipzi-editor','kind':['bin']},'executable':'/a'}
        for messages in ['', '\n'.join(map(json.dumps, [item, {**item,'executable':'/b'}]))]:
            with self.assertRaises(RuntimeError):
                executable(messages, 'zipzi-editor')


class SourceArchive(unittest.TestCase):
    def test_manifest_uses_one_root_and_excludes_models_caches_and_other_checkouts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)/'repo'
            root.mkdir()
            subprocess.run(['git','init','-q',str(root)], check=True)
            for name in ['Cargo.toml','Cargo.lock','crates/core/src/lib.rs','crates/zipzi-editor-core/src/lib.rs',
                         'tools/probe.py','vendor/library/src/lib.rs']:
                path = root/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('source')
            (root/'.gitignore').write_text('target/\n__pycache__/\n*.so\nmodels/local/\n')
            for name in ['models/local/model.json','target/output.rs','crates/core/target/output.rs',
                         'crates/python/native.so','tools/__pycache__/cached.py']:
                path = root/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('must not archive')
            subprocess.run(['git','add','.'],cwd=root,check=True)
            subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.test','commit','-qm','fixture'],cwd=root,check=True)
            new = root/'tools/new.py'
            new.write_text('untracked source')
            manifest = source_manifest(root)
            self.assertEqual(len(manifest),7)
            self.assertIn('Cargo.lock',manifest)
            self.assertIn('tools/new.py',manifest)
            self.assertEqual(len(repository(root)['head']),40)
            outside = Path(tmp)/'outside.rs'
            outside.write_text('private sibling')
            new.unlink()
            new.symlink_to(outside)
            with self.assertRaises(ValueError):
                source_manifest(root)


if __name__ == '__main__':
    unittest.main()
