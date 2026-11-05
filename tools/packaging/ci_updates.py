#!/usr/bin/env python3
"""Validate opt-in Alpha update configuration before starting release builds."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re

from package_updates import private_key

CHANNEL_TAG = 'desktop-alpha-updates'
SIGNING_INPUTS = (
    'ZIPZI_MACOS_CERTIFICATE_P12', 'ZIPZI_MACOS_CERTIFICATE_PASSWORD',
    'ZIPZI_MACOS_CODESIGN_IDENTITY', 'ZIPZI_APPLE_ID', 'ZIPZI_APPLE_TEAM_ID',
    'ZIPZI_APPLE_APP_PASSWORD', 'ZIPZI_WINDOWS_CERTIFICATE_PFX',
    'ZIPZI_WINDOWS_CERTIFICATE_PASSWORD',
)


def configure(env):
    enabled = env.get('ZIPZI_DESKTOP_UPDATES', 'false') or 'false'
    if enabled not in ('true', 'false'):
        raise ValueError('ZIPZI_DESKTOP_UPDATES must be true or false')
    configuration = dict(schema=1, enabled=enabled == 'true')
    if configuration['enabled']:
        repository = env.get('GITHUB_REPOSITORY', '')
        if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
            raise ValueError('Expected GITHUB_REPOSITORY in owner/repo form')
        public_key = env.get('ZIPZI_UPDATE_PUBLIC_KEY', '')
        if not re.fullmatch(r'[0-9a-f]{64}', public_key):
            raise ValueError('ZIPZI_UPDATE_PUBLIC_KEY must be 64 lowercase hex characters')
        key = private_key(env.get('ZIPZI_UPDATE_SIGNING_KEY', ''))
        if key.public_key().public_bytes_raw().hex() != public_key:
            raise ValueError('ZIPZI_UPDATE_SIGNING_KEY does not match ZIPZI_UPDATE_PUBLIC_KEY')
        missing = [name for name in SIGNING_INPUTS if not env.get(name)]
        if missing:
            raise ValueError('Desktop updates require: ' + ', '.join(missing))
        configuration.update(
            url=f'https://github.com/{repository}/releases/download/{CHANNEL_TAG}',
            public_key=public_key,
        )
    # Reuse requires the same public trust settings and enabled/disabled mode.
    identity = hashlib.sha256(json.dumps(configuration, sort_keys=True).encode()).hexdigest()
    return {
        'updates_enabled': enabled,
        'update_url': configuration.get('url', ''),
        'update_public_key': configuration.get('public_key', ''),
        'build_configuration': identity,
        'manifest_artifact': f'alpha-release-manifest-{identity}',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--github-output', type=Path, required=True)
    args = parser.parse_args()
    try:
        outputs = configure(os.environ)
    except ValueError as error:
        parser.error(str(error))
    with args.github_output.open('a', encoding='utf-8') as output:
        for name, value in outputs.items():
            output.write(f'{name}={value}\n')
    print(f'Desktop updates enabled: {outputs["updates_enabled"]}')


if __name__ == '__main__':
    main()
