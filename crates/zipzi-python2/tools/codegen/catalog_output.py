"""Render lazy catalog shards and navigable reference pages."""
import json
import pprint
import inspect
from codegen.reference_routing import SPLITS, page_group
from codegen.model import validate_rules

def render(entries, contract, contract_hash, domains, common, package_version, source_sha, prefix, header, projections=(), introductions=None, rules=None):
    rules={} if rules is None else rules
    validate_rules(rules,tuple(rules))
    for entry in entries:validate_rules(rules,entry.rule_refs)
    entry_index = {entry.path: entry for entry in entries}
    catalog = {entry.path: entry.row() for entry in entries}
    catalog['zipzi']['documentation'] += ' Package '+package_version+'; protocol '+contract['protocol']+'; client source SHA-256 '+source_sha+'.'
    for row in catalog.values():
        row['children'] = tuple(sorted(set(row.get('children', ()))))
        if row.get('fields'):
            row['documentation'] = row['documentation'].partition('\n\nFields:\n')[0]

    def group(path):
        return page_group(path,domains,modeling_domains)

    modeling_domains={entry.path.rsplit('.',1)[-1] for entry in entries
                      if entry.kind=='module' and entry.path.startswith('zipzi.modeling.')
                      and entry.path.count('.')==2}
    groups = sorted({group(path) for path in catalog})
    page_labels={}
    for name in groups:
        refs=sorted({ref for path,row in catalog.items() if group(path)==name for ref in row.get('type_refs',())})
        labels={}
        for ref in refs:
            parts=ref.split('.')
            for size in range(1,len(parts)+1):
                label='.'.join(parts[-size:])
                if sum(other.lower().endswith('.'+label.lower()) or other.lower()==label.lower() for other in refs)==1:break
            labels[ref]=label
        page_labels[name]=labels
    pages = {name: [header.rstrip(), '# '+name.title()+' API', '',
                    'See [common behavior](common.md) for owned-value conventions and inherited methods.', '']
             for name in groups}
    for name,intro in (introductions or {}).items():
        if name in pages:pages[name].extend([intro,''])
    reference = [header.rstrip(), '# Installed Python API reference', '',
                 'Browse the [workflow guide](guide.md) and [common behavior](reference/common.md).', '',
                 'Start with [Session](reference/runtime_session.md), [Edit](reference/runtime_edit.md) and [meshes](reference/meshes.md).', '',
                 '## API domains', '']
    reference.extend('- ['+name.title()+'](reference/'+name+'.md)' for name in groups)
    reference.append('')
    common_page = [header.rstrip(), '# Common API behavior', '']
    for name, doc in sorted(common.items()):
        common_page.extend(['## '+name, '', doc, ''])
    for key, rule in sorted(rules.items()):
        common_page.extend(['## '+key,'',rule.documentation,''])

    # Only combine entries with identical content and relative child navigation.
    canonical = {}
    preferred = {item.canonical_path: 0 if item.projection_id=='canonical' else 1 for item in projections}
    shards = {name: {} for name in groups}
    # Choose reviewed implementation paths before aliases, independent of spelling.
    for path in sorted(catalog, key=lambda path: (preferred.get(path,2), path)):
        row = catalog[path]
        comparable = dict(row)
        comparable['children'] = tuple('@'+c[len(path):] if c.startswith(path+'.') else c for c in row['children'])
        identity = entry_index[path].identity
        key = (identity, json.dumps(comparable, sort_keys=True))
        if identity is not None and key in canonical:
            value = {'alias': canonical[key]}
        else:
            if identity is not None:canonical[key] = path
            value = dict(row)
            inherited = entry_index[path].inherited
            if inherited:
                value.pop('documentation')
                value['common'] = inherited
        shards[group(path)][path] = value

    for path, row in sorted(catalog.items()):
        if entry_index[path].inherited:
            continue
        page = pages[group(path)]
        value = shards[group(path)][path]
        page.extend(['## '+path, ''])
        if 'alias' in value and not (path.startswith(('zipzi.Session.', 'zipzi.Edit.')) and row['kind']=='callable'):
            target = value['alias']
            page.extend(['Alias of ['+target+']('+group(target)+'.md#'+target.lower().replace('.', '')+').', ''])
            continue
        page.extend(['**'+row['kind'].capitalize()+'**', ''])
        if row.get('signature'):page.extend(['`'+row['signature']+'`',''])
        if row.get('documentation'):page.extend([row['documentation'], ''])
        if row.get('rule_refs'):
            page.extend(['See '+', '.join('['+rules[key].title+'](common.md#'+key+')' for key in row['rule_refs'])+'.',''])
        if row.get('type_refs'):
            links=[]
            for ref in row['type_refs']:
                link='['+page_labels[group(path)][ref]+']'
                if group(ref)==group(path):link+='(#'+ref.lower().replace('.','')+')'
                links.append(link)
            page.extend(['Types: '+', '.join(links)+'.',''])
        if row.get('fields'):page.extend(['**'+('Members' if row['kind']=='enum' else 'Fields')+'**',''])
        page.extend('- `'+field+'`: '+text for field, text in row.get('fields', ()))
        page.append('')
        if row.get('example'):
            page.extend(['Example: [example](../'+row['example'].removeprefix('docs/')+').',''])

    for name,page in pages.items():
        refs=sorted({ref for path,row in catalog.items() if group(path)==name for ref in row.get('type_refs',()) if group(ref)!=name})
        if refs:
            # Markdown IDs are page-local; choose the shortest unambiguous suffix.
            labels=page_labels[name]
            declaration=['## Type references','',', '.join('['+labels[ref]+']' for ref in refs), '']
            page[3:3]=declaration
            page.extend('['+labels[ref]+']: '+group(ref)+'.md#'+ref.lower().replace('.','') for ref in refs)
            page.append('')

    # Audit unique type definitions, grouping reexports under one source. An empty
    # summary is an explicit gap even when the type's fields are documented.
    type_groups={}
    for entry in entries:
        if entry.kind in {'type','enum','alias'} or (entry.kind=='property' and entry.documentation_source):
            type_groups.setdefault(entry.identity if entry.identity is not None else entry.path,[]).append(entry)
    missing=[]
    for definitions in type_groups.values():
        if any(entry.summary.strip() for entry in definitions):continue
        paths=sorted(entry.path for entry in definitions)
        source=next((entry.documentation_source for entry in definitions if entry.documentation_source),'')
        missing.append({'identity':definitions[0].identity or definitions[0].path,'paths':paths,'source':source,
                        'pages':sorted({group(path) for path in paths})})
    coverage={'_generated_by':'zipzi-python2/tools/generate.py', 'type_count':len(type_groups),
              'documented_type_count':len(type_groups)-len(missing),
              'missing_type_count':len(missing),'missing':sorted(missing,key=lambda item:item['paths'])}

    catalog_prefix = prefix+'_generated/catalog/'
    from codegen.documentation_audit import quality_report, validate_reference_declarations
    result = {
        prefix+'docs/type-documentation.json': json.dumps(coverage,indent=2,ensure_ascii=False)+'\n',
        prefix+'docs/documentation-quality.json': json.dumps(quality_report(entries,domains,modeling_domains),indent=2,ensure_ascii=False)+'\n',
        prefix+'_generated/metadata.py': header+'"""Protocol and build metadata without help data."""\n'
            +'CONTRACT = '+repr(contract['protocol'])+'\nPACKAGE_VERSION = '+repr(package_version)
            +'\nCLIENT_SOURCE_SHA256 = '+repr(source_sha)+'\nCONTRACT_SHA256 = '
            +repr(contract_hash)+'\n',
        prefix+'docs/reference.md': '\n'.join(reference)+'\n',
        prefix+'docs/reference/common.md': '\n'.join(common_page).rstrip()+'\n',
        catalog_prefix+'__init__.py': header+'"""Lazy offline API directory."""\nfrom .index import lookup as lookup, iter_paths as iter_paths\n',
        catalog_prefix+'common.py': header+'COMMON: dict[str, str] = '+pprint.pformat(common, sort_dicts=True)+'\n',
        catalog_prefix+'rules.py': header+'RULES: dict[str, tuple[str, str, str]] = '+pprint.pformat({key:(r.title,r.documentation,r.source) for key,r in sorted(rules.items())},sort_dicts=True)+'\n',
    }
    for name, page in pages.items():
        result[prefix+'docs/reference/'+name+'.md'] = '\n'.join(page).rstrip()+'\n'
    validate_reference_declarations(result)
    for name, rows in shards.items():
        result[catalog_prefix+name+'.py'] = header+'from .index import Entry\nDATA: dict[str, Entry] = '+pprint.pformat(rows, width=120, sort_dicts=True)+'\n'
    result[catalog_prefix+'index.py'] = (header+'"""Deterministic path routing; shards load only on demand."""\n'
        'from importlib import import_module\nfrom collections.abc import Iterator, Sequence\nfrom typing import Required, TypedDict, cast\n\n'
        'GROUPS = '+repr(tuple(groups))+'\nDOMAINS = '+repr(tuple(domains))+'\nMODELING_DOMAINS = '+repr(tuple(sorted(modeling_domains)))+'\nSPLITS = '+repr(SPLITS)+'''

class Entry(TypedDict, total=False):
    kind: str
    signature: str
    documentation: str
    fields: Sequence[tuple[str, str]]
    children: tuple[str, ...]
    type_refs: tuple[str, ...]
    example: str
    alias: str
    common: str
    rule_refs: tuple[str, ...]
    field_roles: tuple[tuple[str, str, str], ...]

class Resolved(TypedDict, total=False):
    kind: Required[str]
    signature: str
    documentation: str
    fields: Sequence[tuple[str, str]]
    children: tuple[str, ...]
    type_refs: tuple[str, ...]
    example: str
    common: str
    rule_refs: tuple[str, ...]
    rules: tuple[tuple[str, str], ...]
    field_roles: tuple[tuple[str, str, str], ...]
'''+ '\n'+inspect.getsource(page_group)+'''
def _group(path: str) -> str:
    return page_group(path, DOMAINS, MODELING_DOMAINS)

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
            refs = result.get('rule_refs', ())
            if refs:
                from .rules import RULES
                if len(refs) != len(set(refs)) or any(key not in RULES for key in refs):
                    raise ValueError('Invalid documentation rule reference')
                result['rules'] = tuple((RULES[key][0], RULES[key][1]) for key in refs)
            result['children'] = tuple(requested + c[len(path):] if c.startswith(path + '.') else c for c in result.get('children', ()))
            return result
        path = row['alias']

def iter_paths() -> Iterator[str]:
    for path in sorted(path for name in GROUPS for path in _data(name)):
        yield path
''')
    return result
