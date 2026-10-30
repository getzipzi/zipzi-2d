"""Select installed runtime sources and guides for the build fingerprint."""
from collections.abc import Mapping
import hashlib

PREFIX = 'client/python/zipzi/'


def included(path: str) -> bool:
    if not path.startswith(PREFIX):
        return False
    name = path[len(PREFIX):]
    if name == '_generated/metadata.py' or name.startswith('_generated/catalog/') or name == '_generated/catalog.py':
        return False
    if name.endswith('.py'):
        return '__pycache__' not in name.split('/')
    return name.startswith('docs/') and name.endswith('.md') and name != 'docs/reference.md' and not name.startswith('docs/reference/')


def fingerprint(sources: Mapping[str, str]) -> str:
    digest = hashlib.sha256()
    for path, source in sorted(sources.items()):
        if included(path):
            digest.update(path.encode())
            digest.update(b'\0')
            digest.update(source.encode())
    return digest.hexdigest()
