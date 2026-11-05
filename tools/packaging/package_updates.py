#!/usr/bin/env python3
"""Build Velopack desktop packages and authenticate their update feed; never publish."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from release import metadata

ROOT = Path(__file__).resolve().parents[2]
VELOPACK_VERSION = '1.2.161'
PACKAGE_ID = 'Zipzi.Editor'


def private_key(value):
    try:
        return Ed25519PrivateKey.from_private_bytes(bytes.fromhex(value.strip()))
    except ValueError:
        raise ValueError('Expected a 32-byte Ed25519 signing seed encoded as 64 hex characters') from None


def verify_feed_packages(feed):
    """Check actual local payloads before signing or publishing a feed."""
    data = feed.read_bytes()
    entries = json.loads(data)['Assets']
    if not entries:
        raise ValueError('Refusing to sign an empty update feed')
    for asset in entries:
        name = asset['FileName']
        if (not name or name.startswith('.') or not name.isascii()
                or any(not (char.isalnum() or char in '.-_') for char in name)
                or asset['PackageId'] != PACKAGE_ID):
            raise ValueError('Invalid update asset identity or filename')
        package = feed.parent / name
        if package.is_symlink() or not package.is_file() or not 0 < package.stat().st_size <= 2 << 30:
            raise ValueError(f'Missing or invalid update package: {name}')
        with package.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if package.stat().st_size != asset['Size'] or digest.lower() != asset['SHA256'].lower():
            raise ValueError(f'Update package failed verification: {name}')
    return data, entries


def sign_feed(feed, key):
    """Check the actual local payloads before signing the exact feed bytes."""
    data, _ = verify_feed_packages(feed)
    signature = key.sign(data)
    key.public_key().verify(signature, data)
    feed.with_suffix(feed.suffix + '.sig').write_text(signature.hex() + '\n', encoding='ascii')


def package(args):
    info = metadata()
    if args.allow_unsigned and '-alpha.' not in info['version']:
        raise ValueError('--allow-unsigned is restricted to Alpha releases')
    key = private_key(os.environ.get('ZIPZI_UPDATE_SIGNING_KEY', ''))
    expected_key = os.environ.get('ZIPZI_UPDATE_PUBLIC_KEY', '')
    if key.public_key().public_bytes_raw().hex() != expected_key:
        raise ValueError('Signing seed does not match ZIPZI_UPDATE_PUBLIC_KEY used to build Editor')
    update_url = os.environ.get('ZIPZI_UPDATE_URL', '')
    from urllib.parse import urlsplit
    parsed = urlsplit(update_url)
    if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.query or parsed.fragment:
        raise ValueError('Set ZIPZI_UPDATE_URL to the HTTPS update directory used to build Editor')
    binary = args.binary if args.binary else args.app / 'Contents/MacOS/zipzi-editor'
    binary_data = binary.read_bytes()
    if expected_key.encode('ascii') not in binary_data or update_url.encode('utf-8') not in binary_data:
        raise ValueError('Rebuild Editor with this update URL and public key before packaging')
    vpk = ['dotnet', str(args.vpk)] if str(args.vpk).endswith('.dll') else [str(args.vpk)]
    child_env = {name: value for name, value in os.environ.items() if name != 'ZIPZI_UPDATE_SIGNING_KEY'}
    help_text = subprocess.check_output([*vpk, '--help'], text=True, env=child_env)
    actual = re.search(r'Velopack CLI ([^,\s]+)', help_text)
    if actual is None or actual[1] != VELOPACK_VERSION:
        raise ValueError(f'Expected vpk {VELOPACK_VERSION}')
    stage = info['version'].split('-', 1)[1].split('.')[0] if info['prerelease'] else 'stable'
    platform = 'windows-x86_64' if args.binary else 'macos-arm64'
    channel = f'{platform}-{stage}'
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='zipzi-update-') as temporary:
        temporary = Path(temporary)
        output = temporary / 'packages'
        if args.binary:
            if not args.allow_unsigned and not any(os.environ.get(name) for name in
                    ('VPK_SIGN_PARAMS', 'VPK_SIGN_TEMPLATE', 'VPK_AZURE_TRUSTED_SIGN_FILE')):
                raise ValueError('Configure Windows code signing, or use --allow-unsigned for Alpha')
            from package_windows import payload
            content = temporary / 'content'
            content.mkdir()
            for name, data in payload(args.binary, args.revision, preview_dll=args.preview_dll).items():
                path = content / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            extra = ['--runtime', 'win-x64', '--mainExe', 'zipzi-editor.exe', '--noPortable',
                     '--shortcuts', 'StartMenuRoot', '--icon', str(ROOT / 'crates/zipzi-editor/assets/app-icons/zipzi-editor.ico')]
        else:
            if sys.platform != 'darwin':
                raise ValueError('Build macOS update packages on macOS')
            trusted = bool(os.environ.get('VPK_SIGN_APP_IDENTITY')
                           and os.environ.get('VPK_NOTARY_PROFILE'))
            if not trusted and not args.allow_unsigned:
                raise ValueError('Configure macOS signing/notarization, or use --allow-unsigned for Alpha')
            content = args.app.resolve()
            # Quick Look extensions were signed first with their sandbox entitlements.
            extra = ['--runtime', 'osx-arm64', '--mainExe', 'zipzi-editor',
                     '--noInst', '--signDisableDeep']
            if not trusted:
                extra += ['--signAppIdentity', '-']
        command = [*vpk, 'pack', '--packId', PACKAGE_ID,
                   '--packVersion', info['version'], '--packTitle', 'Zipzi Editor',
                   '--packDir', str(content), '--outputDir', str(output),
                   '--channel', channel, '--yes', '--skip-updates', *extra]
        subprocess.run(command, cwd=ROOT, check=True, env=child_env)
        feed = output / f'releases.{channel}.json'
        sign_feed(feed, key)
        names = [asset['FileName'] for asset in json.loads(feed.read_bytes())['Assets']]
        names += [feed.name, feed.name + '.sig']
        if args.binary:
            installers = list(output.glob('*Setup.exe'))
            if len(installers) != 1:
                raise ValueError('Expected one Velopack Setup.exe')
            installer = f'Zipzi-Editor-{info["tag"]}-windows-x86_64.exe'
            shutil.copy2(installers[0], args.output / installer)
        else:
            archives = list(output.glob('*Portable.zip'))
            if len(archives) != 1:
                raise ValueError('Expected one Velopack portable app archive')
            unpacked = temporary / 'unpacked'
            subprocess.run(['ditto', '-xk', str(archives[0]), str(unpacked)], check=True)
            apps = list(unpacked.glob('*.app'))
            if len(apps) != 1:
                raise ValueError('Expected one packaged app')
            from package_macos import package as dmg
            dmg(apps[0], args.output / f'Zipzi-Editor-{info["tag"]}-macos-arm64.dmg', trusted=trusted)
        for name in names:
            shutil.copy2(output / name, args.output / name)
    print(f'PASS {platform}: installer and signed {channel} update feed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument('--binary', type=Path)
    inputs.add_argument('--app', type=Path)
    parser.add_argument('--preview-dll', type=Path)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--vpk', default='vpk')
    parser.add_argument('--allow-unsigned', action='store_true', help='Allow Alpha packages without OS developer signing; feed signing is still required')
    args = parser.parse_args()
    package(args)


if __name__ == '__main__':
    main()
