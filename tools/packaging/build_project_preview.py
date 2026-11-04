#!/usr/bin/env python3
"""Build static project Shell previews with the host platform's native compiler."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def static_library(profile, target_dir=None):
    command = ['cargo', 'build', '--locked', '-p', 'zipzi-project-preview',
               '--profile', 'dev' if profile == 'debug' else 'release', '--message-format=json-render-diagnostics']
    if target_dir:
        command += ['--target-dir', str(target_dir)]
    result = subprocess.run(command, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True)
    for line in result.stdout.splitlines():
        item = json.loads(line)
        if item.get('reason') == 'compiler-artifact' and item['target']['name'] == 'zipzi_project_preview':
            for name in item['filenames']:
                if name.endswith('.lib' if sys.platform == 'win32' else '.a'):
                    return Path(name)
    raise RuntimeError('Cargo did not produce the preview static library')


def extension_plist(bundle_id, version, build, thumbnail):
    attributes = {'QLSupportedContentTypes': ['dev.zipzi.project']}
    if thumbnail:
        attributes['QLThumbnailMinimumDimension'] = 0
    else:
        attributes['QLSupportsSearchableItems'] = False
        attributes['QLIsDataBasedPreview'] = True
    return dict(CFBundleIdentifier=bundle_id, CFBundleName='Zipzi Thumbnail' if thumbnail else 'Zipzi Preview',
                CFBundleExecutable='zipzi-preview', CFBundlePackageType='XPC!',
                CFBundleShortVersionString=version, CFBundleVersion=build,
                LSMinimumSystemVersion='12.0', NSExtension=dict(
                    NSExtensionPointIdentifier='com.apple.quicklook.thumbnail' if thumbnail else 'com.apple.quicklook.preview',
                    NSExtensionPrincipalClass='ZipziThumbnailProvider' if thumbnail else 'ZipziPreviewProvider',
                    NSExtensionAttributes=attributes))


def build_macos(app, library, identity='-'):
    metadata = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    for thumbnail in (True, False):
        name = 'ZipziThumbnail' if thumbnail else 'ZipziPreview'
        contents = app / 'Contents/PlugIns' / (name + '.appex') / 'Contents'
        (contents / 'MacOS').mkdir(parents=True, exist_ok=True)
        info = extension_plist(metadata['CFBundleIdentifier'] + '.' + name,
                               metadata.get('CFBundleShortVersionString', '0.1.0'),
                               metadata['CFBundleVersion'], thumbnail)
        (contents / 'Info.plist').write_bytes(plistlib.dumps(info))
        subprocess.run(['xcrun', 'clang', '-fobjc-arc', '-fapplication-extension', '-O2',
                        '-mmacosx-version-min=12.0', '-DZIPZI_THUMBNAIL=' + str(int(thumbnail)),
                        '-I', str(ROOT / 'crates/zipzi-project-preview/include'),
                        str(ROOT / 'native/project-preview/macos/Preview.m'), str(library),
                        '-framework', 'Cocoa', '-framework', 'Quartz', '-framework', 'QuickLookThumbnailing',
                        '-framework', 'Security', '-framework', 'UniformTypeIdentifiers',
                        '-liconv', '-Wl,-e,_NSExtensionMain',
                        '-o', str(contents / 'MacOS/zipzi-preview')], check=True)
        entitlements = contents / 'Resources/entitlements.plist'
        entitlements.parent.mkdir(exist_ok=True)
        entitlements.write_bytes(plistlib.dumps({'com.apple.security.app-sandbox': True}))
        subprocess.run(['codesign', '--force', '--sign', identity, '--entitlements', str(entitlements),
                        str(contents.parent)], check=True)


def build_windows(output, library):
    # Run in an x64 MSVC developer environment. No self-registration inside Explorer.
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    command = ['cl', '/nologo', '/std:c++17', '/O2', '/EHsc', '/MD', '/LD', '/DUNICODE', '/D_UNICODE',
                    '/I' + str(ROOT / 'crates/zipzi-project-preview/include'),
                    str(ROOT / 'native/project-preview/windows/Preview.cpp'), str(library),
                    '/Fo' + str(output.with_suffix('.obj')), '/link', '/OUT:' + str(output),
                    '/DEF:' + str(ROOT / 'native/project-preview/windows/Preview.def'),
                    'ole32.lib', 'windowscodecs.lib', 'gdi32.lib', 'user32.lib', 'uuid.lib',
                    'msimg32.lib', 'advapi32.lib', 'bcrypt.lib', 'ntdll.lib', 'userenv.lib',
                    'ws2_32.lib', 'dbghelp.lib']
    windows_compile(command, output)


def windows_compile(command, output):
    if shutil.which('cl'):
        subprocess.run(command, check=True)
    else:
        # Match the x64 MSVC toolchain used by Rust, including on CI runners.
        finder = Path(os.environ.get('ProgramFiles(x86)', r'C:\Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        installation = subprocess.check_output([str(finder), '-latest', '-products', '*',
            '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'], text=True).strip()
        setup = Path(installation) / 'VC/Auxiliary/Build/vcvars64.bat'
        if not installation or not setup.is_file():
            raise RuntimeError('Install x64 MSVC C++ Build Tools and Windows SDK')
        # Fixed developer environment setup plus an argv-quoted compiler invocation.
        script = output.with_suffix('.cmd')
        script.write_text('@echo off\ncall "' + str(setup) + '" >nul\nif errorlevel 1 exit /b %errorlevel%\n'
                          + subprocess.list2cmdline(command) + '\n', encoding='utf-8')
        try:
            subprocess.run(['cmd.exe', '/d', '/c', str(script)], check=True)
        finally:
            script.unlink(missing_ok=True)


def test_windows(dll):
    directory = dll.parent / 'project-preview-test'
    directory.mkdir(parents=True, exist_ok=True)
    subprocess.run([sys.executable, str(ROOT / 'tools/runtime/project_preview_fixture.py'), str(directory)], check=True)
    runner = directory / 'provider-test.exe'
    windows_compile(['cl', '/nologo', '/std:c++17', '/EHsc', '/MD', '/DUNICODE', '/D_UNICODE',
                     str(ROOT / 'native/project-preview/windows/ProviderTest.cpp'),
                     '/Fo' + str(runner.with_suffix('.obj')), '/Fe' + str(runner),
                     '/link', 'ole32.lib', 'shlwapi.lib', 'gdi32.lib', 'user32.lib', 'uuid.lib'], runner)
    subprocess.run([str(runner), str(dll), str(directory)], check=True)


def test_macos(library, directory):
    directory.mkdir(parents=True, exist_ok=True)
    subprocess.run([sys.executable, str(ROOT / 'tools/runtime/project_preview_fixture.py'), str(directory)], check=True)
    for thumbnail in (True, False):
        runner = directory / ('thumbnail-provider-test' if thumbnail else 'preview-provider-test')
        subprocess.run(['xcrun', 'clang', '-fobjc-arc', '-O2', '-DZIPZI_THUMBNAIL=' + str(int(thumbnail)),
                        '-I', str(ROOT / 'crates/zipzi-project-preview/include'),
                        str(ROOT / 'native/project-preview/macos/ProviderTest.m'), str(library),
                        '-framework', 'Cocoa', '-framework', 'Quartz', '-framework', 'QuickLookThumbnailing',
                        '-framework', 'Security', '-framework', 'UniformTypeIdentifiers',
                        '-liconv', '-o', str(runner)], check=True)
        subprocess.run([str(runner), str(directory)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path)
    parser.add_argument('--dll', type=Path)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    parser.add_argument('--target-dir', type=Path)
    parser.add_argument('--identity', default=os.environ.get('ZIPZI_CODESIGN_IDENTITY', '-'))
    parser.add_argument('--test', action='store_true', help='Run native provider smoke checks after building')
    args = parser.parse_args()
    if sys.platform == 'darwin' and args.app:
        library = static_library(args.profile, args.target_dir)
        build_macos(args.app.resolve(), library, args.identity)
        if args.test:
            test_macos(library, args.app.resolve().parent / 'project-preview-test')
    elif sys.platform == 'win32' and args.dll:
        build_windows(args.dll, static_library(args.profile, args.target_dir))
        if args.test:
            test_windows(args.dll.resolve())
    else:
        parser.error('Use --app on macOS or --dll on Windows (x64 MSVC environment)')


if __name__ == '__main__':
    main()
