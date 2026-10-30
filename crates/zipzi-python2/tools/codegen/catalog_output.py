"""Render lazy catalog shards and navigable reference pages."""
import hashlib
import json
import pprint


def render(catalog, contract, domains, common, package_version, source_sha, prefix, header):
    catalog['zipzi']['documentation'] += ' Package '+package_version+'; protocol '+contract['protocol']+'; client source SHA-256 '+source_sha+'.'
    for row in catalog.values():
        row['children'] = tuple(sorted(set(row.get('children', ()))))
        if row.get('fields'):
            row['documentation'] = row['documentation'].partition('\n\nFields:\n')[0]

    def group(path):
        segment = path.split('.')[1] if '.' in path else ''
        return segment if segment in domains else 'runtime'

    groups = sorted({group(path) for path in catalog})
    pages = {name: [header.rstrip(), '# '+name.title()+' API', '',
                    'See [common behavior](common.md) for inherited owned-value methods.', '']
             for name in groups}
    for path, row in sorted(catalog.items()):
        method = path.rsplit('.', 1)[-1]
        if method in common and row['documentation'] == common[method]:
            continue
        page = pages[group(path)]
        page.extend(['## '+path, '', '`'+row.get('signature', '')+'`', '', row.get('documentation', ''), ''])
        page.extend('- `'+field+'`: '+text for field, text in row.get('fields', ()))
        page.append('')
    reference = [header.rstrip(), '# Installed Python API reference', '',
                 'Browse the [workflow guide](guide.md) and [common behavior](reference/common.md).', '',
                 '## API domains', '']
    reference.extend('- ['+name.title()+'](reference/'+name+'.md)' for name in groups)
    reference.append('')
    common_page = [header.rstrip(), '# Common owned-value behavior', '']
    for name, doc in sorted(common.items()):
        common_page.extend(['## '+name, '', doc, ''])

    # Only combine entries with identical content and relative child navigation.
    canonical = {}
    shards = {name: {} for name in groups}
    for path, row in sorted(catalog.items()):
        comparable = dict(row)
        comparable['children'] = tuple('@'+c[len(path):] if c.startswith(path+'.') else c for c in row['children'])
        key = json.dumps(comparable, sort_keys=True)
        if key in canonical:
            value = {'alias': canonical[key]}
        else:
            canonical[key] = path
            value = dict(row)
            method = path.rsplit('.', 1)[-1]
            if method in common and value['documentation'] == common[method]:
                value.pop('documentation')
                value['common'] = method
        shards[group(path)][path] = value

    catalog_prefix = prefix+'_generated/catalog/'
    result = {
        prefix+'_generated/metadata.py': header+'"""Protocol and build metadata without help data."""\n'
            +'CONTRACT = '+repr(contract['protocol'])+'\nPACKAGE_VERSION = '+repr(package_version)
            +'\nCLIENT_SOURCE_SHA256 = '+repr(source_sha)+'\nCONTRACT_SHA256 = '
            +repr(hashlib.sha256(json.dumps(contract, sort_keys=True).encode()).hexdigest())+'\n',
        prefix+'docs/reference.md': '\n'.join(reference)+'\n',
        prefix+'docs/reference/common.md': '\n'.join(common_page).rstrip()+'\n',
        catalog_prefix+'__init__.py': header+'"""Lazy offline API directory."""\nfrom .index import lookup as lookup, iter_paths as iter_paths\n',
        catalog_prefix+'common.py': header+'COMMON: dict[str, str] = '+pprint.pformat(common, sort_dicts=True)+'\n',
    }
    for name, page in pages.items():
        result[prefix+'docs/reference/'+name+'.md'] = '\n'.join(page).rstrip()+'\n'
    for name, rows in shards.items():
        result[catalog_prefix+name+'.py'] = header+'from .index import Entry\nDATA: dict[str, Entry] = '+pprint.pformat(rows, width=120, sort_dicts=True)+'\n'
    result[catalog_prefix+'index.py'] = (header+'"""Deterministic path routing; shards load only on demand."""\n'
        'from importlib import import_module\nfrom collections.abc import Iterator, Sequence\nfrom typing import Required, TypedDict, cast\n\n'
        'GROUPS = '+repr(tuple(groups))+'\nDOMAINS = '+repr(tuple(domains))+'''

class Entry(TypedDict, total=False):
    kind: str
    signature: str
    documentation: str
    fields: Sequence[tuple[str, str]]
    children: tuple[str, ...]
    example: str
    alias: str
    common: str

class Resolved(TypedDict, total=False):
    kind: Required[str]
    signature: str
    documentation: str
    fields: Sequence[tuple[str, str]]
    children: tuple[str, ...]
    example: str
    common: str

def _group(path: str) -> str:
    segment = path.split('.')[1] if '.' in path else ''
    return segment if segment in DOMAINS else 'runtime'

def _data(name: str) -> dict[str, Entry]:
    return cast(dict[str, Entry], import_module('.' + name, __package__).DATA)

def lookup(path: str) -> Resolved:
    requested = path
    seen: set[str] = set()
    while True:
        if path in seen or len(seen) >= 64:
            raise ValueError('Catalog alias cycle')
        seen.add(path)
        row = _data(_group(path))[path]
        if 'alias' not in row:
            result = cast(Resolved, dict(row))
            if 'common' in result:
                from .common import COMMON
                result['documentation'] = COMMON[result['common']]
                del result['common']
            result['children'] = tuple(requested + c[len(path):] if c.startswith(path + '.') else c for c in result.get('children', ()))
            return result
        path = row['alias']

def iter_paths() -> Iterator[str]:
    for path in sorted(path for name in GROUPS for path in _data(name)):
        yield path
''')
    return result
