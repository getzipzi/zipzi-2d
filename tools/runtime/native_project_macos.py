"""Opt-in LaunchServices acceptance against a real macOS Editor executable.

Run with ./tools/dev-python tools/runtime/native_project_macos.py --help.
Creates an isolated development bundle and private host directories, opens only
copies of repository fixtures, and terminates only the host it started. Does not
change the default application unless --finder-default is selected, which uses
a separate guardian process to restore the previous handler on exit. Optional
dialog checks require Accessibility access and English native panel labels.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import select
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def wait_for(check, description, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(.05)
    raise TimeoutError(description)


def read_json(path):
    try:
        value = json.loads(path.read_text())
        return value.get('data', value)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def window_count(pid):
    script = ("ObjC.import('CoreGraphics'); JSON.stringify(ObjC.deepUnwrap("
              "ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0)))"
              f".filter(w => Number(w.kCGWindowOwnerPID) === {pid}).length)")
    return int(subprocess.check_output(
        ['osascript', '-l', 'JavaScript', '-e', script], text=True, timeout=10))


def make_bundle(binary, directory, *, finder_default=False):
    bundle = directory / 'Zipzi Native Acceptance.app'
    contents = bundle / 'Contents'
    (contents / 'MacOS').mkdir(parents=True)
    (contents / 'Resources').mkdir()
    shutil.copy2(binary, contents / 'MacOS/zipzi-editor')
    shutil.copy2(ROOT / 'crates/zipzi-editor/assets/app-icons/zipzi-editor-icon-vector.icns',
                 contents / 'Resources/AppIcon.icns')
    shutil.copy2(ROOT / 'crates/zipzi-editor/assets/app-icons/Assets.car',
                 contents / 'Resources/Assets.car')
    spec = importlib.util.spec_from_file_location('release', ROOT / 'tools/packaging/release.py')
    release = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(release)
    metadata = dict(CFBundleIdentifier='dev.zipzi.native-acceptance.' + uuid.uuid4().hex,
        CFBundleName='Zipzi Native Acceptance', CFBundleExecutable='zipzi-editor',
        CFBundlePackageType='APPL', CFBundleVersion='1', CFBundleIconFile='AppIcon', CFBundleIconName='ZipziEditor',
        NSHighResolutionCapable=True,
        **release.project_document_types(development=not finder_default))
    (contents / 'Info.plist').write_bytes(plistlib.dumps(metadata))
    return bundle


class DefaultHandler:
    """The child owns restoration; closing its pipe also works on parent crash."""
    def __init__(self, bundle):
        self.bundle = bundle
        self.child = subprocess.Popen(['swift', str(Path(__file__).with_name(
            'project_file_handler.swift')), str(bundle)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, text=True)
        self.evidence = None
        try:
            if not select.select([self.child.stdout], [], [], 30)[0]:
                raise TimeoutError('temporary default handler readiness')
            self.evidence = json.loads(self.child.stdout.readline())
            assert self.evidence['active'] == self.evidence['temporary'], self.evidence
        except BaseException:
            self.close()
            raise

    def close(self):
        self.child.stdin.close()
        # communicate() must not try to flush the closed pipe.
        self.child.stdin = None
        output, _ = self.child.communicate(timeout=20)
        # Spotlight can re-register an Owner-ranked .app left on disk. This is
        # our throwaway bundle; remove it before checking effective restoration.
        if self.bundle.exists():
            shutil.rmtree(self.bundle)
        if self.evidence is not None:
            restored = json.loads(output)
            def actual_handler():
                actual = json.loads(subprocess.check_output(['swift', str(Path(__file__).with_name(
                    'project_file_handler.swift')), '--query'], text=True, timeout=10))
                return actual if actual['active'] == self.evidence['previous'] else None
            restored['verified_after_unregister'] = wait_for(actual_handler, 'previous default handler restored')
            self.evidence['restoration'] = restored
            assert self.child.returncode == 0, restored


def accessibility(pid, body, *values):
    script = ('on run argv\n'
              'set targetPID to item 1 of argv as integer\n'
              'tell application "System Events"\n'
              'tell (first application process whose unix id is targetPID)\n'
              'set frontmost to true\n' + body + '\nend tell\nend tell\nend run')
    return subprocess.check_output(['osascript', '-e', script, str(pid), *values],
                                   text=True, timeout=10).strip()


def check_dialogs(pid, output, runtime, original):
    import zipfile

    def save_panel(name=None):
        accessibility(pid, 'keystroke "s" using {command down, shift down}')
        wait_for(lambda: accessibility(pid, 'return exists window "Save"') == 'true', 'Save panel')
        if name is not None:
            accessibility(pid, 'set value of text field "Save As:" of splitter group 1 '
                          'of window "Save" to item 2 of argv\n'
                          'click button "Save" of splitter group 1 of window "Save"', name)

    def cancel_save():
        accessibility(pid, 'click button "Cancel" of splitter group 1 of window "Save"')
        wait_for(lambda: accessibility(pid, 'return exists window "Save"') == 'false', 'Save closed')

    def content(path):
        with zipfile.ZipFile(path) as archive:
            return (json.loads(archive.read('project.json')),
                    {name: archive.read(name) for name in archive.namelist() if name.startswith('assets/')})

    save_panel()
    suggested = accessibility(pid, 'return value of text field "Save As:" '
                              'of splitter group 1 of window "Save"')
    assert suggested == original.stem + '.zipzi', suggested
    location = accessibility(pid, 'return value of pop up button "Where:" '
                             'of splitter group 1 of window "Save"')
    assert location == output.name, location
    cancel_save()
    checks = dict(suggested_name_and_parent_directory=True, save_cancel=True)

    # Other suffixes must produce explicit correction before any publication.
    save_panel('wrong.png')
    wait_for(lambda: accessibility(pid, 'return exists sheet 1 of window "Save"') == 'true',
             'wrong extension must prompt explicitly')
    warning = accessibility(pid, 'return value of static text 1 of sheet 1 of window "Save"')
    assert '.png' in warning and '.zipzi' in warning, warning
    accessibility(pid, 'click button "Cancel" of sheet 1 of window "Save"')
    cancel_save()
    assert not (output / 'wrong.png').exists() and not (output / 'wrong.png.zipzi').exists()
    checks['wrong_extension_requires_explicit_correction'] = True

    target = output / 'overwrite.zipzi'
    shutil.copy2(ROOT / 'fixtures/observe_inspection/projects/rotated-warp.zipzi', target)
    old = target.read_bytes()
    save_panel('overwrite')
    wait_for(lambda: accessibility(pid, 'return exists sheet 1 of window "Save"') == 'true',
             'normalized target must prompt for replacement')
    accessibility(pid, 'click button "Cancel" of sheet 1 of window "Save"')
    cancel_save()
    assert target.read_bytes() == old
    assert original in {info.path for info in runtime.sessions.list()}
    checks['normalized_overwrite_cancel_preserves_target_and_session'] = True

    for name in ('saved without extension', 'saved uppercase.ZIPZI', 'saved twice.zipzi.zipzi'):
        destination = output / (name if Path(name).suffix else name + '.zipzi')
        save_panel(name)
        wait_for(lambda: destination in {info.path for info in runtime.sessions.list()}, 'Save As completed')
        assert content(destination) == content(original)
    checks['missing_uppercase_and_double_extension_save_full_document_and_resources'] = True
    accessibility(pid, 'keystroke "o" using command down')
    wait_for(lambda: accessibility(pid, 'return exists window "Open"') == 'true', 'Open panel')
    accessibility(pid, 'key code 53')
    wait_for(lambda: accessibility(pid, 'return exists window "Open"') == 'false', 'Open closed')
    checks['open_panel_cancel'] = True
    return checks


def run_case(bundle, output, mode, dialogs=False, finder_default=False):
    import zipzi

    output.mkdir()
    projects = []
    for name in ('角色 one.zipzi', '-角色 two.ZIPZI', 'same document copy.zipzi'):
        path = output / name
        shutil.copy2(ROOT / 'fixtures/observe_inspection/projects/pixel-binding.zipzi', path)
        projects.append(path)
    original_hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in projects}
    result = dict(mode=mode, checks={})
    pid = None
    # A short path is required by Unix-domain socket limits.
    with tempfile.TemporaryDirectory(prefix='zipzi-native-', dir='/tmp') as private:
        private = Path(private)
        variables = dict(ZIPZI_RUNTIME_DIR=str(private / 'runtime'),
            ZIPZI_RUNTIME_PROFILE='acceptance', ZIPZI_CONFIG_DIR=str(output / 'config'),
            ZIPZI_CACHE_DIR=str(output / 'cache'), ZIPZI_DEV_SESSION_DIR=str(output / 'session'),
            ZIPZI_DEV_PID_FILE=str(output / 'host.pid'), ZIPZI_DEV_READY_FILE=str(output / 'ready'),
            ZIPZI_DEV_BACKGROUND='1')
        environment = {k: v for k, v in os.environ.items()
                       if not k.startswith(('ZIPZI_', 'MTL_HUD'))}
        environment.update(variables)
        guard = None
        if finder_default:
            # Finder starts the cold host without a shell environment.
            plist = bundle / 'Contents/Info.plist'
            metadata = plistlib.loads(plist.read_bytes())
            metadata['LSEnvironment'] = variables
            plist.write_bytes(plistlib.dumps(metadata))
        def launch(paths=(), arguments=(), new=False):
            if finder_default and paths:
                script = ('on run argv\nset projectFiles to {}\n'
                          'repeat with p in argv\nset end of projectFiles to (POSIX file (contents of p)) as alias\nend repeat\n'
                          'tell application "Finder"\nactivate\nreveal projectFiles\n'
                          'set selection to projectFiles\nend tell\n'
                          'tell application "System Events" to tell application process "Finder" '
                          'to keystroke "o" using command down\nend run')
                subprocess.run(['osascript', '-e', script, *map(str, paths)],
                               check=True, timeout=15)
                return
            command = ['open', '-a', str(bundle)]
            if '--no-window' in arguments:
                command += ['-g']
            if new:
                command += ['-n']
            if new:
                for key, value in variables.items():
                    command += ['--env', f'{key}={value}']
                command += ['--stdout', str(output / 'application.log'),
                            '--stderr', str(output / 'application.log')]
            command += [str(p) for p in paths]
            if arguments:
                command += ['--args', *arguments]
            subprocess.run(command, env=environment, check=True, timeout=15)
        try:
            if finder_default:
                guard = DefaultHandler(bundle)
                result['default_handler'] = guard.evidence
                def file_handlers_ready():
                    actual = json.loads(subprocess.check_output(['swift', str(Path(__file__).with_name(
                        'project_file_handler.swift')), '--query', *map(str, projects)], text=True, timeout=10))
                    return actual if actual['applications'] == [str(bundle)] * len(projects) else None
                result['default_handler']['file_lookup'] = wait_for(file_handlers_ready, 'all files use the test bundle')
                accessibility_script = ('on run argv\nrepeat with p in argv\n'
                    'set projectFile to (POSIX file (contents of p)) as alias\n'
                    'tell application "Finder" to update projectFile\n'
                    'end repeat\nend run')
                subprocess.run(['osascript', '-e', accessibility_script, *map(str, projects)],
                               check=True, timeout=10)
            if mode == 'resident':
                launch(arguments=('--no-window',), new=True)
            else:
                launch(projects[:2], new=True)
                # A second OS event is sent before waiting for host readiness.
                early = not (output / 'ready').exists()
                launch(projects[2:])
                result['checks']['event_sent_before_ready'] = early
            pid = int(wait_for(lambda: (output / 'host.pid').read_text()
                              if (output / 'host.pid').exists() else None, 'host PID'))
            wait_for(lambda: (output / 'ready').exists(), 'Runtime readiness')
            with zipzi.connect(endpoint=private / 'runtime/acceptance.sock') as runtime:
                assert runtime.connection.host_pid == pid, 'attached to a different host'
                if mode == 'resident':
                    assert window_count(pid) == 0, 'resident created a window'
                    result['checks']['initial_window_count'] = 0
                    launch(projects)
                def opened():
                    infos = runtime.sessions.list()
                    ready = {info.path: info for info in infos if info.state == 'ready'}
                    return ready if set(ready) == set(projects) and len(infos) == 3 else None
                sessions = wait_for(opened, 'three distinct archive paths must be ready')
                wait_for(lambda: window_count(pid) > 0, 'file event must open a window')
                result['checks']['distinct_paths_same_document_uuid'] = len(sessions)
                result['checks']['window_created'] = True
                identity = {str(path): info.id.serial for path, info in sessions.items()}
                launch(projects[:1])
                def active():
                    saved = read_json(output / 'session/session.json')
                    return saved if saved and saved['active'] == str(projects[0]) else None
                wait_for(active, 'repeated file event must activate the original document')
                assert {str(p): i.id.serial for p, i in opened().items()} == identity
                result['checks']['repeat_event_preserves_sessions_and_activates'] = True
                # Invoke a second production process, exercising the IPC path too.
                forwarded = output / 'forwarded.pid'
                subprocess.run([str(bundle / 'Contents/MacOS/zipzi-editor'), '--',
                    str(projects[1])], env=dict(environment, ZIPZI_DEV_PID_FILE=str(forwarded)),
                    check=True, timeout=20, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
                assert forwarded.read_text() == f'reused:{pid}'
                wait_for(lambda: (read_json(output / 'session/session.json') or {}).get('active')
                         == str(projects[1]), 'IPC must activate the requested document')
                assert {str(p): i.id.serial for p, i in opened().items()} == identity
                result['checks']['second_process_reuses_host'] = True
                for path, info in sessions.items():
                    session = runtime.sessions.attach(info.id)
                    assert not session.project.diagnose().wait().diagnostics
                    assert len(session.meshes.list().items) == 2
                    session.detach()
                result['checks']['all_resources_valid_and_two_meshes_per_project'] = True
                recent = wait_for(lambda: read_json(output / 'config/recent-files.json'), 'recent files')
                assert set(recent['paths']) == set(map(str, projects))
                result['checks']['recent_files_are_archives'] = True
                if dialogs and mode == 'resident':
                    result['dialogs'] = check_dialogs(pid, output, runtime, projects[1])
            assert {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in projects} == original_hashes
            result['checks']['input_bytes_unchanged'] = True
        finally:
            try:
                stop_owned_host(bundle, output, pid)
            finally:
                if guard is not None:
                    guard.close()
    return result


def stop_owned_host(bundle, output, pid):
    # Verify ownership again before signalling; never stop another Editor.
    if pid is None and (output / 'host.pid').exists():
        pid = int((output / 'host.pid').read_text())
    if pid is not None:
        command = subprocess.run(['ps', '-p', str(pid), '-o', 'command='],
                                 capture_output=True, text=True, check=False).stdout.strip()
        if command.startswith(str(bundle / 'Contents/MacOS/zipzi-editor')):
            os.kill(pid, signal.SIGTERM)
            wait_for(lambda: subprocess.run(['ps', '-p', str(pid)],
                stdout=subprocess.DEVNULL).returncode != 0, 'owned host exit', timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--dialogs', action='store_true', help='exercise English native panels via Accessibility')
    parser.add_argument('--finder-default', action='store_true',
        help='temporarily select this bundle as the .zipzi default and open via Finder; restore on exit')
    parser.add_argument('--case', choices=('cold', 'resident'),
        help='run one case; otherwise run both specified-application cases')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('requires macOS and a usable native desktop')
    if args.finder_default and args.case != 'cold':
        parser.error('--finder-default currently requires --case cold; resident Finder acceptance is unfinished')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    bundle = make_bundle(args.binary.resolve(strict=True), output, finder_default=args.finder_default)
    results = []
    try:
        for mode in (args.case,) if args.case else ('cold', 'resident'):
            result = run_case(bundle, output / mode, mode, args.dialogs, args.finder_default)
            results.append(result)
            print(json.dumps(result, ensure_ascii=False), flush=True)
    finally:
        (output / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2) + '\n')
        # Remove only this throwaway bundle from LaunchServices registration.
        if bundle.exists():
            subprocess.run(['/System/Library/Frameworks/CoreServices.framework/Frameworks/'
                'LaunchServices.framework/Support/lsregister', '-u', str(bundle)], check=False)


if __name__ == '__main__':
    main()
