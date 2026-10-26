#!/usr/bin/env python3
"""Package an already-signed Editor app as a verified drag-to-Applications DMG."""
import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile

from release import versions

INSTALL = '''Install Zipzi Editor

1. Drag Zipzi Editor.app onto the Applications folder.
2. Eject this disk image.
3. Open Zipzi Editor from Applications.

This Alpha is ad hoc signed and is not notarized by Apple. If macOS blocks the
first launch and you trust this download, open System Settings > Privacy &
Security and choose Open Anyway after attempting to open the installed app.

安装 Zipzi Editor

1. 将 Zipzi Editor.app 拖入 Applications（应用程序）文件夹。
2. 推出此安装磁盘。
3. 从“应用程序”打开 Zipzi Editor。

此 Alpha 版本尚未经过 Apple 公证。如果首次启动被阻止，且你信任此下载，
请在尝试打开已安装的 App 后，进入“系统设置 → 隐私与安全性”选择“仍要打开”。
'''


def signature(app):
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)


def binary_digest(app):
    with (app / 'Contents/MacOS/zipzi-editor').open('rb') as binary:
        return hashlib.file_digest(binary, 'sha256').hexdigest()


def package(app, output):
    app, output = app.resolve(), output.resolve()
    if app.name != 'Zipzi Editor.app' or output.suffix != '.dmg':
        raise ValueError('Expected Zipzi Editor.app and a .dmg output path')
    if output.exists():
        raise ValueError(f'Output already exists: {output}')
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if info['CFBundleIdentifier'] != 'dev.zipzi.editor':
        raise ValueError('Expected the Zipzi Editor bundle identifier')
    version = versions(info['ZipziReleaseVersion'])['version']
    signature(app)
    digest = binary_digest(app)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='zipzi-installer-') as directory:
        temporary = Path(directory)
        stage, mount = temporary / 'contents', temporary / 'mounted'
        stage.mkdir()
        mount.mkdir()
        subprocess.run(['ditto', str(app), str(stage / app.name)], check=True)
        (stage / 'Applications').symlink_to('/Applications')
        (stage / 'Install.txt').write_text(INSTALL, encoding='utf-8')
        image = temporary / output.name
        subprocess.run(['hdiutil', 'create', '-quiet', '-fs', 'HFS+', '-format', 'UDZO',
                        '-volname', 'Zipzi Editor ' + version, '-srcfolder', str(stage), str(image)], check=True)
        subprocess.run(['hdiutil', 'verify', '-quiet', str(image)], check=True)
        subprocess.run(['hdiutil', 'attach', '-quiet', '-readonly', '-nobrowse',
                        '-mountpoint', str(mount), str(image)], check=True)
        try:
            if os.readlink(mount / 'Applications') != '/Applications':
                raise ValueError('Installer Applications link is invalid')
            installed = mount / app.name
            signature(installed)
            if binary_digest(installed) != digest or (installed / 'Contents/Info.plist').read_bytes() != (app / 'Contents/Info.plist').read_bytes():
                raise ValueError('Installer changed the app executable or release identity')
        finally:
            subprocess.run(['hdiutil', 'detach', '-quiet', str(mount)], check=True)
        shutil.copyfile(image, output)
    print(f'PASS installer: {output}; disk image, Applications link and original app signature verified')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        package(args.app, args.output)
    except (ValueError, KeyError) as error:
        parser.error(str(error))


if __name__ == '__main__':
    main()
