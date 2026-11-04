#!/usr/bin/env python3
"""Create and verify a portable x86-64 Windows Editor archive."""
import argparse
import hashlib
from pathlib import Path
import struct
import zipfile

from release import metadata

ROOT = Path(__file__).resolve().parents[2]
NOTICES = ('LICENSE-GPL-3.0', 'LICENSE-MIT', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md')


def check_executable(data):
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('Editor executable is not a PE file')
    offset = struct.unpack_from('<I', data, 60)[0]
    if offset + 6 > len(data) or data[offset:offset + 4] != b'PE\0\0':
        raise ValueError('Editor executable has an invalid PE header')
    if struct.unpack_from('<H', data, offset + 4)[0] != 0x8664:
        raise ValueError('Editor executable must target Windows x86-64')


def package(binary, output, revision, root=ROOT, preview_dll=None):
    info = metadata(root)
    data = binary.read_bytes()
    check_executable(data)
    if preview_dll is None:
        raise ValueError('A built x64 preview DLL is required')
    preview = preview_dll.read_bytes()
    check_executable(preview)
    if not revision or any(c not in '0123456789abcdef' for c in revision.lower()) or len(revision) != 40:
        raise ValueError('Expected a full 40-character source commit SHA')
    files = {'zipzi-editor.exe': data,
             'ZipziProjectPreview.dll': preview,
             'Register-ProjectPreview.ps1': (Path(__file__).parent / 'Register-ProjectPreview.ps1').read_bytes(),
             'Register-FileAssociation.ps1': (Path(__file__).parent / 'Register-FileAssociation.ps1').read_bytes(),
             'FILE-ASSOCIATION.txt': (
                 'Run PowerShell from this directory:\r\n'
                 '  powershell -ExecutionPolicy Bypass -File .\\Register-FileAssociation.ps1\r\n'
                 'Unregister this location:\r\n'
                 '  powershell -ExecutionPolicy Bypass -File .\\Register-FileAssociation.ps1 -Unregister\r\n'
                 'Select Zipzi Editor in Windows Open With / Default apps. Moving the portable app requires registering again.\r\n'
                 'Registration also enables static thumbnails and the Explorer preview pane.\r\n'
             ).encode()}
    for name in NOTICES:
        files[name] = (root / name).read_bytes()
    vendor = root / 'vendor'
    files['vendor-notices/README.md'] = (vendor / 'README.md').read_bytes()
    for path in vendor.rglob('*'):
        if path.is_file() and path.name.lower().startswith(('license', 'copying', 'notice')):
            files['vendor-notices/' + path.relative_to(vendor).as_posix()] = path.read_bytes()
    files['BUILD-INFO.txt'] = (f'Zipzi Editor {info["version"]}\nSource commit: {revision}\n'
                               'Platform: Windows x86-64\n').encode()
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for name, content in sorted(files.items()):
            archive.writestr('Zipzi Editor/' + name, content)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None or len(archive.namelist()) != len(files):
            raise ValueError('Windows archive failed validation')
        for name, content in files.items():
            if hashlib.sha256(archive.read('Zipzi Editor/' + name)).digest() != hashlib.sha256(content).digest():
                raise ValueError(f'Windows archive content changed: {name}')
    print(f'PASS Windows portable archive: {output}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--preview-dll', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    args = parser.parse_args()
    package(args.binary, args.output, args.revision, preview_dll=args.preview_dll)


if __name__ == '__main__':
    main()
