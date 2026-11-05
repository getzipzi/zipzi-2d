#!/usr/bin/env python3
"""Advance the fixed GitHub Alpha update release without replacing package bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

from ci_updates import CHANNEL_TAG
from package_updates import PACKAGE_ID, verify_feed_packages
from release import VERSION

CHANNELS = ('macos-arm64-alpha', 'windows-x86_64-alpha')


def alpha_version(version):
    match = VERSION.fullmatch(version)
    if not match or match[4] != 'alpha':
        raise ValueError('Only X.Y.Z-alpha.N versions may advance the Alpha update feed')
    return tuple(int(match[index]) for index in (1, 2, 3, 5))


def read_feed(path, public_key, channel):
    data = path.read_bytes()
    signature = path.with_suffix('.json.sig').read_text(encoding='ascii').strip()
    try:
        if len(data) > 2 << 20 or not re.fullmatch(r'[0-9a-fA-F]{128}', signature):
            raise ValueError('Invalid feed or signature size')
        public_key.verify(bytes.fromhex(signature), data)
    except (ValueError, InvalidSignature):
        raise ValueError(f'Invalid signed update feed: {path.name}') from None
    assets = json.loads(data)['Assets']
    full_versions = []
    for asset in assets:
        name = asset['FileName']
        kind = asset['Type'].lower()
        if (asset['PackageId'] != PACKAGE_ID or kind not in ('full', 'delta')
                or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name)
                or not name.endswith(f'-{channel}-{kind}.nupkg')
                or not 0 < asset['Size'] <= 2 << 30
                or not re.fullmatch(r'[0-9a-fA-F]{64}', asset['SHA256'])):
            raise ValueError(f'Invalid update asset in {path.name}')
        version = alpha_version(asset['Version'])
        if kind == 'full':
            full_versions.append(version)
    if not full_versions:
        raise ValueError(f'Missing full update package in {path.name}')
    return max(full_versions), assets


class GitHub:
    def __init__(self, repository):
        self.repository = repository

    def run(self, *args):
        return subprocess.run(['gh', *args, '--repo', self.repository], check=True,
                              capture_output=True, text=True, encoding='utf-8')

    def release(self):
        result = subprocess.run(
            ['gh', 'api', f'repos/{self.repository}/releases/tags/{CHANNEL_TAG}'],
            capture_output=True, text=True, encoding='utf-8',
        )
        if result.returncode:
            if '(HTTP 404)' in result.stderr:
                return None
            raise RuntimeError('Cannot read the existing Alpha update release: ' + result.stderr)
        return json.loads(result.stdout)

    def create(self, revision):
        self.run('release', 'create', CHANNEL_TAG, '--target', revision, '--prerelease',
                 '--latest=false', '--title', 'Zipzi Desktop Alpha updates', '--notes',
                 'Authenticated Alpha update feeds and immutable desktop packages. '
                 'For installers and release notes, use the versioned Alpha releases.')

    def download(self, names, destination):
        args = ['release', 'download', CHANNEL_TAG, '--dir', str(destination)]
        for name in names:
            args += ['--pattern', name]
        self.run(*args)

    def upload(self, files, replace=False):
        if files:
            self.run('release', 'upload', CHANNEL_TAG, *map(str, files),
                     *(['--clobber'] if replace else []))


def publish(directory, repository, revision, tag, key, github=None):
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('Expected repository in owner/repo form')
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('Expected a full source commit SHA')
    if not tag.startswith('v'):
        raise ValueError('Expected an Alpha version tag')
    version = alpha_version(tag[1:])
    public_key = Ed25519PublicKey.from_public_bytes(bytes.fromhex(key))
    feeds = [directory / f'releases.{channel}.json' for channel in CHANNELS]
    if set(directory.glob('releases.*.json')) != set(feeds):
        raise ValueError('Expected exactly the macOS and Windows Alpha feeds')
    packages = {}
    for channel, feed in zip(CHANNELS, feeds):
        latest, _ = read_feed(feed, public_key, channel)
        if latest != version:
            raise ValueError(f'{feed.name} does not match release {tag}')
        _, assets = verify_feed_packages(feed)
        packages.update((asset['FileName'], asset) for asset in assets)

    github = github or GitHub(repository)
    release = github.release()
    if release and (release.get('immutable') or release.get('draft')
                    or not release.get('prerelease')):
        raise ValueError('The fixed Alpha update release must be a published, mutable prerelease')
    existing = {asset['name']: asset for asset in release['assets']} if release else {}
    with tempfile.TemporaryDirectory(prefix='zipzi-publish-updates-') as temporary:
        remote = Path(temporary)
        for channel, feed in zip(CHANNELS, feeds):
            signature = feed.with_suffix('.json.sig')
            present = [name for name in (feed.name, signature.name) if name in existing]
            if len(present) == 1:
                # Recover an interrupted feed/signature replacement on a rerun.
                # Every package version already uploaded also guards against rollback below.
                continue
            if present:
                github.download(present, remote)
                try:
                    previous, _ = read_feed(remote / feed.name, public_key, channel)
                except ValueError:
                    # GitHub replaces the feed/signature as separate assets. Only
                    # repair a mixed pair containing this exact authenticated build.
                    if ((remote / feed.name).read_bytes() != feed.read_bytes()
                            and (remote / signature.name).read_bytes() != signature.read_bytes()):
                        raise
                    continue
                if previous > version:
                    print('Newer Alpha update feed is already published; leaving it unchanged.')
                    return False
                if previous == version and (remote / feed.name).read_bytes() != feed.read_bytes():
                    raise ValueError(f'Refusing to replace different feed bytes for {tag}')

        # Even an interrupted feed update cannot let an older tag replace newer packages.
        for name in existing:
            match = re.fullmatch(r'Zipzi\.Editor-(.+)-(?:macos-arm64|windows-x86_64)-alpha-full\.nupkg', name)
            if match and alpha_version(match[1]) > version:
                print('Newer Alpha packages are already published; leaving the feeds unchanged.')
                return False

        missing = []
        for name, asset in packages.items():
            if name not in existing:
                missing.append(directory / name)
                continue
            recorded = existing[name].get('digest')
            expected = 'sha256:' + asset['SHA256'].lower()
            if not recorded:
                github.download([name], remote)
                with (remote / name).open('rb') as stream:
                    recorded = 'sha256:' + hashlib.file_digest(stream, 'sha256').hexdigest()
            if recorded.lower() != expected:
                raise ValueError(f'Refusing to overwrite immutable update package: {name}')
        if release is None:
            github.create(revision)
        github.upload(missing)
        # Publish only after all referenced packages are present. Exact signed bytes are retained.
        github.upload([path for feed in feeds for path in (feed, feed.with_suffix('.json.sig'))],
                      replace=True)
    print(f'PASS {CHANNEL_TAG}: authenticated Alpha feeds advanced to {tag}')
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--repository', required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--public-key', required=True)
    args = parser.parse_args()
    publish(args.input, args.repository, args.revision, args.tag, args.public_key)


if __name__ == '__main__':
    main()
