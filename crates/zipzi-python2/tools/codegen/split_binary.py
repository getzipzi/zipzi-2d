"""Split direct binary adapters by reviewed type domain."""
import ast
from collections import defaultdict
from codegen.split_modeling import _name, _uses


def split(source, graph, header):
    tree=ast.parse(source)
    lines=source.splitlines(keepends=True)
    owners={_name(identity).lower():domain for identity,domain in graph.domains.items()}
    owners.update({name:'common' for name in ('version','sourceref','imageid','planid')})
    # Scene edit unions are an independent authoring concern from scene values.
    owners.update({name:'scene_edits' for name in ('sceneoperation', 'sceneedit',
                  'scenecontentedit', 'organizationedit', 'reparentedit', 'attachmentedit')})
    functions={}
    groups=defaultdict(list)
    support=[]
    entry_domains={'encode_operations':'commands','decode_records':'queries',
                   'decode_observation':'observation','decode_spatial':'spatial',
                   'decode_preview':'preview','decode_samples':'sampling',
                   'decode_meshes':'mesh','decode_bindings':'bindings','decode_images':'resources'}
    support_functions={'_field','_mapping','_point','_mesh_meshing'}
    for node in tree.body:
        if isinstance(node,ast.FunctionDef) and node.name.startswith(('_read_','_write_')):
            name=node.name.split('_',2)[2]
            if name not in owners:raise ValueError('Unowned binary adapter: '+name)
            functions[node.name]=owners[name]
            groups[owners[name]].append(node)
        elif isinstance(node,ast.FunctionDef) and node.name in entry_domains:
            groups[entry_domains[node.name]].append(node)
        elif isinstance(node,ast.FunctionDef) and node.name in support_functions:
            support.append(node)
        elif isinstance(node,ast.ClassDef) and node.name in {'Writer','Reader'}:
            support.append(node)
        elif isinstance(node,(ast.Assign,ast.AnnAssign)):
            support.append(node)
        elif isinstance(node,(ast.Import,ast.ImportFrom)):
            pass
        else:raise ValueError('Unreviewed binary declaration: '+type(node).__name__)
    prefix='client/python/zipzi/_generated/binary/'
    imports={
        'Mapping':'from collections.abc import Mapping',
        'cast':'from typing import cast',
        'UUID':'from uuid import UUID',
        'math':'import math',
        'struct':'import struct',
        'ops':'from ... import _operations as ops',
        'meshes':'from ... import meshes',
        'resources':'from ... import resources',
        'm':'from .. import values as m',
        'scene':'from .. import scene',
        'Source':'from ..._values import Source',
        'Version':'from ..._values import Version',
        'Snapshot':'from ..._results import Snapshot',
        'record':'from ..._values import record',
        'sequence':'from ..._values import sequence',
        'string':'from ..._values import string',
        'integer':'from ..._values import integer',
        'number':'from ..._values import number',
        'ExtensionValue':'from ..._extensions import ExtensionValue',
    }
    support_names={'_field','_mapping','_point','_mesh_meshing','Writer','Reader',
                   'MAX_BYTES','MAX_ITEMS','INPUT_SIGNATURE','MESH_SIGNATURE','BINDING_SIGNATURE',
                   'SAMPLE_SIGNATURE','OUTPUT_SIGNATURE'}
    def text(node):
        first=min([node.lineno,*[n.lineno for n in getattr(node,'decorator_list',())]])
        return ''.join(lines[first-1:node.end_lineno]).rstrip()
    def external(names):return sorted({imports[name] for name in names if name in imports})
    output={}
    used=_uses(support)
    support_exports=sorted(node.name for node in support if isinstance(node,(ast.FunctionDef,ast.ClassDef)))
    output[prefix+'support.py']='\n'.join([header.rstrip(),'from __future__ import annotations',*external(used),'',*(text(node)+'\n' for node in support),'__all__ = '+repr(support_exports)]).rstrip()+'\n'
    for domain,nodes in sorted(groups.items()):
        used=_uses(nodes)
        if 'sequence' in used and not any(isinstance(call,ast.Call) and isinstance(call.func,ast.Name) and call.func.id=='sequence' for node in nodes for call in ast.walk(node)):
            used.remove('sequence')
        parts=[header.rstrip(),'from __future__ import annotations',*external(used)]
        if any(any(name in functions and functions[name]!=domain for name in _uses([node])) for node in nodes):
            parts.append('from functools import cache')
        required=sorted(used & support_names)
        if required:parts.append('from .support import '+', '.join(name+' as '+name for name in required))
        parts.append('')
        for node in nodes:
            dependencies=sorted(name for name in _uses([node]) if name in functions and functions[name]!=domain)
            original=text(node).splitlines()
            if dependencies:
                loader='_dependencies_'+node.name
                parts.extend(['@cache', 'def '+loader+'():',
                              *['    from .'+functions[name]+' import '+name+' as '+name for name in dependencies],
                              '    return '+', '.join(dependencies)+',', ''])
                original.insert(1,'    '+', '.join(dependencies)+', = '+loader+'()')
            parts.extend(['\n'.join(original),''])
        parts.append('__all__ = '+repr(sorted(node.name for node in nodes)))
        output[prefix+domain+'.py']='\n'.join(parts).rstrip()+'\n'
    init=[header.rstrip(),'"""Direct binary adapters by semantic domain."""']
    for name,domain in sorted(entry_domains.items()):init.append('from .'+domain+' import '+name+' as '+name)
    output[prefix+'__init__.py']='\n'.join(init)+'\n'
    return output
