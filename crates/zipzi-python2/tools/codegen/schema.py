"""Serialize and safely replay the compiler's complete logical type graph."""
import hashlib
import json
from pathlib import Path, PurePosixPath

from . import normalize, projections

OWNER = 'zipzi-python2/tools/generate.py'


def digest(logical):
    return hashlib.sha256(json.dumps(logical, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()


def serialize(logical):
    grouped = {}
    for name, definition in sorted(logical['definitions'].items()):
        domain = projections.OWNERS.get(name, 'external')
        grouped.setdefault(domain, {})[name] = definition
    files, shards = {}, []
    for domain, definitions in sorted(grouped.items()):
        path = 'types/'+domain+'.json'
        data = (json.dumps({'_generated_by': OWNER, 'definitions': definitions}, indent=2, sort_keys=True, ensure_ascii=False)+'\n').encode()
        files['schema/'+path] = data.decode()
        shards.append({'path': path, 'sha256': hashlib.sha256(data).hexdigest()})
    index = {'_generated_by': OWNER, 'format_version': 2, 'root': logical['root'],
             'shards': shards, 'contract_sha256': digest(logical)}
    files['schema/authoring.json'] = json.dumps(index, indent=2, sort_keys=True, ensure_ascii=False)+'\n'
    return files


def _unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('Duplicate JSON key: '+key)
        result[key] = value
    return result


def read(path):
    path = Path(path)
    data = json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=_unique)
    if data.get('format_version') != 2:
        raise ValueError('Unsupported schema format')
    if 'definitions' in data:
        if set(data) != {'format_version', 'root', 'definitions'}:
            raise ValueError('Invalid logical contract')
        normalize.validate_graph(data['root'], data['definitions'], projections.EXTERNAL_TYPES)
        return data
    if (path.is_symlink() or data.get('_generated_by') != OWNER
            or not isinstance(data.get('shards'), list)):
        raise ValueError('Invalid schema index')
    definitions, paths = {}, set()
    for shard in data['shards']:
        relative = shard['path']
        if not isinstance(relative, str):
            raise ValueError('Invalid schema shard path: '+str(relative))
        parts = PurePosixPath(relative)
        if (parts.as_posix() != relative or len(parts.parts) != 2
                or parts.parts[0] != 'types' or parts.suffix != '.json'
                or parts.parts[1] in {'.', '..'} or relative in paths):
            raise ValueError('Invalid schema shard path: '+str(relative))
        paths.add(relative)
        target = path.parent.joinpath(*parts.parts)
        if any(path.parent.joinpath(*parts.parts[:i]).is_symlink() for i in range(1, len(parts.parts)+1)) or not target.resolve().is_relative_to(path.parent.resolve()):
            raise ValueError('Schema shard traverses a symlink')
        raw = target.read_bytes()
        if hashlib.sha256(raw).hexdigest() != shard['sha256']:
            raise ValueError('Schema shard digest mismatch: '+relative)
        value = json.loads(raw, object_pairs_hook=_unique)
        if value.get('_generated_by') != OWNER or not isinstance(value.get('definitions'), dict):
            raise ValueError('Invalid schema shard: '+relative)
        for name, definition in value['definitions'].items():
            if name in definitions:
                raise ValueError('Duplicate type definition: '+name)
            if projections.OWNERS.get(name, 'external') != parts.stem:
                raise ValueError('Wrong schema domain: '+name)
            definitions[name] = definition
    logical = {'format_version': 2, 'root': data['root'], 'definitions': definitions}
    if digest(logical) != data.get('contract_sha256'):
        raise ValueError('Logical contract digest mismatch')
    normalize.validate_graph(logical['root'], definitions, projections.EXTERNAL_TYPES)
    return logical
