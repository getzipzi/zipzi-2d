"""Project generated modeling declarations and codecs into semantic domains."""
import ast
from collections import defaultdict
import re
from codegen.validation import PRIMITIVES


def _name(identity):
    if '<' in identity:
        return ''.join(re.findall(r'(?:^|::)([A-Z][A-Za-z0-9_]*)',identity))
    return identity.rsplit('::',1)[-1]


def _uses(nodes):
    used = {node.id for statement in nodes for node in ast.walk(statement)
            if isinstance(node,ast.Name) and isinstance(node.ctx,ast.Load)}
    for statement in nodes:
        for node in ast.walk(statement):
            if (isinstance(node,ast.Call) and isinstance(node.func,ast.Name)
                    and node.func.id == 'cast' and node.args
                    and isinstance(node.args[0],ast.Constant) and isinstance(node.args[0].value,str)):
                annotation=ast.parse(node.args[0].value,mode='eval')
                used.update(n.id for n in ast.walk(annotation) if isinstance(n,ast.Name))
    return used


def split(source, graph, header):
    tree=ast.parse(source)
    lines=source.splitlines(keepends=True)
    owners={}
    for identity,domain in graph.domains.items():
        name=_name(identity)
        owners[name]=domain
        for variant in graph.definitions[identity].get('variants',()):
            owners[name+variant['rust_name']]=domain
    owners.update({'SourceRef':'common','Version':'common'})
    values=defaultdict(list)
    codecs=defaultdict(list)
    symbols={}
    function_domains={}
    support_functions={'_bytes','_boolean',*('_check_'+kind for kind in PRIMITIVES)}
    for node in tree.body:
        if isinstance(node,ast.ClassDef):
            if node.name not in owners:raise ValueError('Unowned generated class: '+node.name)
            symbols[node.name]=owners[node.name]
            values[owners[node.name]].append(node)
        elif isinstance(node,ast.AnnAssign) and isinstance(node.target,ast.Name) and 'TypeAlias' in ast.unparse(node.annotation):
            name=node.target.id
            if name not in owners:raise ValueError('Unowned generated union: '+name)
            symbols[name]=owners[name]
            values[owners[name]].append(node)
        elif isinstance(node,ast.FunctionDef) and node.name.startswith(('validate_','encode_','read_')):
            name=node.name.split('_',1)[1]
            if name not in owners:raise ValueError('Unowned generated codec: '+node.name)
            domain=owners[name]
            if domain=='scene' and name in {'SceneOperation','SceneEdit','SceneContentEdit',
                                            'OrganizationEdit','ReparentEdit','AttachmentEdit'}:
                domain='scene_edits'
            function_domains[node.name]=domain
            codecs[domain].append(node)
        elif isinstance(node,ast.Assign) and len(node.targets)==1:
            target=node.targets[0]
            if isinstance(target,ast.Attribute) and target.attr=='__module__' and isinstance(target.value,ast.Name):
                values[owners[target.value.id]].append(node)
            elif isinstance(target,ast.Name) and target.id=='OPERATION_TYPES':
                values['commands'].append(node)
            elif isinstance(target,ast.Name) and target.id=='__all__':
                pass
            else:raise ValueError('Unreviewed modeling assignment: '+ast.unparse(target))
        elif isinstance(node,(ast.Import,ast.ImportFrom)):
            pass
        elif isinstance(node,ast.FunctionDef) and node.name in support_functions:
            pass
        else:raise ValueError('Unreviewed modeling declaration: '+type(node).__name__)

    domains=sorted(values)
    output={}
    prefix='client/python/zipzi/_generated/'
    prelude={
        'dataclass':'from dataclasses import dataclass',
        'field':'from dataclasses import field',
        'Enum':'from enum import Enum',
        'MappingProxyType':'from types import MappingProxyType',
        'NamedTuple':'from typing import NamedTuple',
        'TypeAlias':'from typing import TypeAlias',
        'cast':'from typing import cast',
        '_ClassVar':'from typing import ClassVar as _ClassVar',
        '_Mapping':'from collections.abc import Mapping as _Mapping',
        'Mapping':'from collections.abc import Mapping',
        '_Version':'from ..._values import Version as _Version',
        'Version':'from ..._values import Version',
        '_Source':'from ..._values import Source as _Source',
        '_Bytes':'from builtins import bytes as _Bytes',
        'CompactValue':'from ..._presentation import CompactValue',
        'ExtensionValue':'from ..._extensions import ExtensionValue',
        '_freeze_extension':'from ..._extensions import freeze as _freeze_extension',
        '_validate_extension':'from ..._extensions import validate as _validate_extension',
        '_encode_extension':'from ..._extensions import encode as _encode_extension',
        '_read_extension':'from ..._extensions import read as _read_extension',
        'record':'from ..._values import record',
        'sequence':'from ..._values import sequence',
        'string':'from ..._values import string',
        'integer':'from ..._values import integer',
        'number':'from ..._values import number',
        'math':'import math',
        '_bytes':'from .._modeling_support import _bytes',
        '_boolean':'from .._modeling_support import _boolean',
    }
    prelude.update({name:'from .._modeling_support import '+name for name in support_functions if name.startswith('_check_')})
    def text(node):
        first=min([node.lineno,*[item.lineno for item in getattr(node,'decorator_list',())]])
        return ''.join(lines[first-1:node.end_lineno]).rstrip()
    def imports(names):return sorted({prelude[name] for name in names if name in prelude})
    support=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in support_functions]
    support_names=_uses(support)
    output[prefix+'_modeling_support.py']=header+'from __future__ import annotations\n'+'\n'.join(imports(support_names)).replace('from ..._values','from .._values')+'\n\n'+'\n\n'.join(text(n) for n in support)+'\n\n__all__ = '+repr(sorted(support_functions))+'\n'
    for domain,nodes in sorted(values.items()):
        used=_uses(nodes)
        external={name:symbols[name] for name in used if name in symbols and symbols[name]!=domain}
        default_reads=sorted(name for name in used if name.startswith('read_') and name in function_domains)
        missing=[name for name in used if name.startswith('read_') and name not in function_domains]
        if missing:raise ValueError('Unknown modeling default readers: '+', '.join(sorted(missing)))
        parts=[header.rstrip(),'from __future__ import annotations',*imports(used)]
        if external:
            parts+=['from typing import TYPE_CHECKING','if TYPE_CHECKING:']
            for name,owner in sorted(external.items()):parts+=['    from .'+owner+' import '+name]
        parts+=['']
        for reader in default_reads:
            target=reader[5:]
            parts += ['def '+reader+'(value: object) -> '+target+':',
                      '    from ..codecs.'+function_domains[reader]+' import '+reader+' as decode',
                      '    return decode(value)','']
        for node in nodes:
            piece=text(node).replace('"zipzi.modeling"','"zipzi.modeling.'+domain+'"')
            parts.extend([piece,''])
        output[prefix+'values/'+domain+'.py']='\n'.join(parts).rstrip()+'\n'
    init=[header.rstrip(),'"""Canonical immutable values, explicitly exported by domain."""']
    for name,domain in sorted(symbols.items()):init.append('from .'+domain+' import '+name+' as '+name)
    init.append('from ..._extensions import ExtensionValue as ExtensionValue')
    init.append('from .commands import OPERATION_TYPES as OPERATION_TYPES')
    output[prefix+'values/__init__.py']='\n'.join(init)+'\n'
    for domain,nodes in sorted(codecs.items()):
        used=_uses(nodes)
        names=sorted(name for name in used if name in symbols)
        parts=[header.rstrip(),'from __future__ import annotations',*imports(used)]
        if names:parts.append('from ..values import '+', '.join(names))
        parts.append('')
        for node in nodes:
            used_in_function=_uses([node])
            dependencies=sorted(name for name in used_in_function if name in function_domains and function_domains[name]!=domain)
            original=text(node).splitlines()
            for name in reversed(dependencies):
                original.insert(1,'    from .'+function_domains[name]+' import '+name)
            parts.extend(['\n'.join(original),''])
        output[prefix+'codecs/'+domain+'.py']='\n'.join(parts).rstrip()+'\n'
    init=[header.rstrip(),'"""Direct typed JSON codecs, exported only as domain modules."""']
    # Preserve eager domain initialization so shrinking the package exports does
    # not defer file loading into the first authoring operation.
    init.extend('from . import '+domain for domain in sorted(codecs))
    init.append('__all__ = '+repr(sorted(codecs)))
    output[prefix+'codecs/__init__.py']='\n'.join(init)+'\n'
    for domain,nodes in sorted(values.items()):
        exported=sorted(name for name,owner in symbols.items() if owner==domain)
        used=_uses(nodes)
        annotation_names=sorted(name for name in used if name in symbols and symbols[name]!=domain)
        parts=[header.rstrip(),'"""Canonical '+domain+' modeling values."""']
        for name in exported:parts.append('from .._generated.values.'+domain+' import '+name+' as '+name)
        for name in annotation_names:parts.append('from .._generated.values.'+symbols[name]+' import '+name+' as '+name)
        # Private names used by postponed annotations must resolve via __module__.
        private_types=[name for name in ('_ClassVar','_Mapping','_Version','_Source','_Bytes') if name in used]
        for name in private_types:
            parts.append(prelude[name].replace('from ...','from ..'))
        if private_types:
            parts.append('_ANNOTATION_TYPES = ('+', '.join(private_types)+',)')
        if 'ExtensionValue' in used:parts.append('from .._extensions import ExtensionValue as ExtensionValue')
        parts.append('__all__ = '+repr(exported))
        output['client/python/zipzi/modeling/'+domain+'.py']='\n'.join(parts)+'\n'
    root=[header.rstrip(),'"""Core modeling commands, queries and domain modules."""']
    root += ['from . import '+domain+' as '+domain for domain in domains]
    core=['ModelingCommand','ModelingQuery','OwnedModelingRecords']
    for name in core:
        if name in symbols:root.append('from .'+symbols[name]+' import '+name+' as '+name)
    root.append('__all__ = '+repr(domains+core))
    output['client/python/zipzi/modeling/__init__.py']='\n'.join(root)+'\n'
    return output, symbols, function_domains
