"""macOS Seatbelt policy for the isolated v4 agent task packet."""

import json
from pathlib import Path


def policy(packet: Path, python_base: Path, temporary: Path) -> str:
    def sub(path):
        return '(subpath ' + json.dumps(str(path)) + ')'

    parents = ' '.join('(literal ' + json.dumps(str(path)) + ')'
                       for path in set(packet.parents) | set(python_base.parents))
    readable = '(require-any ' + sub(packet) + ' ' + sub(python_base) + ' ' + parents + ')'
    writable = '(require-any ' + sub(packet / 'work') + ' ' + sub(packet / 'output') + ' ' + \
               sub(packet / 'host') + ' ' + sub(temporary) + \
               ' (literal "/dev/null") (literal "/dev/tty"))'
    blocked = '(require-any ' + ' '.join(sub(path) for path in
              ('/Users', '/private/tmp', '/private/var/folders', '/Volumes', '/opt')) + ')'
    return '\n'.join([
        '(version 1)', '(allow default)',
        '(deny file-read-data (require-all ' + blocked + ' (require-not ' + readable + ')))',
        '(deny file-write* (require-not ' + writable + '))',
        '(deny network-inbound)',
        '(deny network-outbound (require-not (remote unix-socket)))',
    ])
