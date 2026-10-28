"""Editor development build, publication and watcher; platform lifecycle is shared."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import time
import uuid

from dev_lifecycle import Lifecycle, save
from dev_platform import file_lock, executable_name
from dev_process import matching
from workspace_tools import executable

ROOT = Path(__file__).resolve().parents[2]


class Editor:
    def __init__(self, root=ROOT):
        self.root = Path(root).resolve()
        self.state = self.root / 'target/dev-editor'
        self.lifecycle = Lifecycle(self.state)
        self.bundle = self.root / 'target/Zipzi Editor Dev.app'
        self.binary = (self.bundle / 'Contents/MacOS/zipzi-editor' if sys.platform == 'darwin'
                       else self.state / executable_name('zipzi-editor'))
        self.env = dict(os.environ, ZIPZI_DEV_BACKGROUND='1',
                        ZIPZI_DEV_CONTROL_DIR=str(self.state),
                        ZIPZI_DEV_SESSION_DIR=str(self.root / 'target/dev-session-zipzi-editor'),
                        ZIPZI_EDITOR_HOST=str(self.binary), MTL_HUD_ENABLED='1',
                        MTL_HUD_ALIGNMENT='bottomleft', MTL_HUD_SCALE='0.16')

    def build(self):
        # Separate from tests. An explicit dev override is caller-relative.
        target = Path(os.environ.get('ZIPZI_DEV_TARGET_DIR', str(self.root / 'target/dev'))).resolve()
        command = ['cargo', 'build', '--locked', '-p', 'zipzi-editor', '--target-dir', str(target),
                   '--message-format=json-render-diagnostics']
        with (self.state / 'build.jsonl').open('w', encoding='utf-8') as output, (self.state / 'build.log').open('w', encoding='utf-8') as errors:
            result = subprocess.run(command, cwd=self.root, env=self.env, stdout=output, stderr=errors)
        if result.returncode:
            print((self.state / 'build.log').read_text(encoding='utf-8', errors='replace')[-8000:], file=sys.stderr)
            raise subprocess.CalledProcessError(result.returncode, command)
        return [str(executable((self.state / 'build.jsonl').read_text(encoding='utf-8'), 'zipzi-editor'))]

    def guard(self, owned):
        untracked = [item for item in matching(self.binary) if item not in owned]
        if untracked:
            raise RuntimeError(f'Untracked development instances: {[item.pid for item in untracked]}; close them before reload')

    def publish(self, command):
        self.binary.parent.mkdir(parents=True, exist_ok=True)
        temporary = self.binary.with_name(self.binary.name + '.' + uuid.uuid4().hex)
        try:
            shutil.copy2(command[0], temporary)
            os.replace(temporary, self.binary)
        finally:
            temporary.unlink(missing_ok=True)
        if sys.platform == 'darwin':
            resources = self.bundle / 'Contents/Resources'
            resources.mkdir(parents=True, exist_ok=True)
            shutil.copy2(self.root / 'LICENSE-GPL-3.0', resources / 'LICENSE')
            icon = self.root / 'crates/zipzi-editor/assets/app-icons/zipzi-editor-icon-vector.icns'
            if icon.exists():
                shutil.copy2(icon, resources / 'AppIcon.icns')
            with (self.bundle / 'Contents/Info.plist').open('wb') as stream:
                plistlib.dump(dict(CFBundleIdentifier='dev.zipzi.editor.watch', CFBundleName='Zipzi Editor Dev',
                    CFBundleExecutable='zipzi-editor', CFBundlePackageType='APPL', CFBundleVersion='0.1.0',
                    CFBundleIconFile='AppIcon', NSHighResolutionCapable=True), stream)
        elif sys.platform == 'win32':
            # The executable embeds UI assets, its icon and the GPUI DPI manifest.
            # Keep the applicable licenses beside the development portable binary.
            for name in ('LICENSE-GPL-3.0', 'LICENSE-MIT', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md'):
                shutil.copy2(self.root / name, self.binary.parent / name)
        return [str(self.binary)]

    def request_stop(self, owned):
        if sys.platform != 'win32':
            return
        for identity in owned:
            if identity.process() is None:
                continue
            request = self.state / f'quit-{identity.pid}.json'
            response = self.state / f'quit-{identity.pid}.reply.json'
            nonce = uuid.uuid4().hex
            save(request, {'nonce': nonce})
            deadline = time.monotonic() + 15
            try:
                while identity.process() is not None:
                    try:
                        result = json.loads(response.read_text(encoding='utf-8'))
                    except (FileNotFoundError, json.JSONDecodeError):
                        result = {}
                    if result.get('nonce') == nonce and result.get('error'):
                        raise RuntimeError(result['error'])
                    if time.monotonic() >= deadline:
                        raise TimeoutError('Editor did not confirm a safe exit; the running process was preserved')
                    time.sleep(.05)
            finally:
                request.unlink(missing_ok=True)
                response.unlink(missing_ok=True)

    def reload(self):
        result = self.lifecycle.reload(self.build, cwd=self.root, env=self.env,
            timeout=float(os.environ.get('ZIPZI_DEV_START_TIMEOUT', '15')),
            publish=self.publish, guard=self.guard, before_stop=self.request_stop)
        print(f"Resident host {result['reused']} reused" if 'reused' in result else f"Editor ready: {result['owned'].pid}")
        return result

    def snapshot(self):
        paths = [self.root / 'Cargo.toml', self.root / 'Cargo.lock']
        for directory in ('crates', 'vendor', 'tools'):
            paths.extend(path for path in (self.root / directory).rglob('*')
                         if path.suffix in {'.rs', '.toml', '.lock', '.metal', '.wgsl', '.svg', '.icns', '.ico', '.rc', '.manifest', '.py'})
        result = {}
        for path in paths:
            try:
                stat = path.stat()
                result[str(path)] = (stat.st_mtime_ns, stat.st_size)
            except FileNotFoundError:
                pass
        return result

    def watch(self):
        with file_lock(self.state / 'watch.lock', blocking=False):
            previous = self.snapshot()
            result = self.reload()
            if 'reused' in result:
                return 0
            while self.lifecycle.active():
                time.sleep(.5)
                current = self.snapshot()
                if current != previous:
                    previous = current
                    try:
                        result = self.reload()
                        if 'reused' in result:
                            return 0
                    except BlockingIOError:
                        # Another reload is in progress; retry this change next tick.
                        previous = {}
                    except (subprocess.CalledProcessError, RuntimeError, TimeoutError) as error:
                        print(error, file=sys.stderr)
            print('Editor closed; stopping watcher')
        return 0


def main(args=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', nargs='?', default='watch', choices=('watch', 'reload', 'run', 'stop'))
    mode = parser.parse_args(args).mode
    editor = Editor()
    if mode == 'stop':
        editor.lifecycle.stop(before_stop=editor.request_stop)
        return 0
    if mode == 'watch':
        return editor.watch()
    result = editor.reload()
    if mode == 'run' and 'owned' in result:
        process = result['owned'].process()
        return process.wait() if process else 0
    return 0
