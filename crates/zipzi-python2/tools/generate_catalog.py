"""Deterministic offline API metadata, sourced from bindings, Rust fields and source docs."""
import ast
import re
import pprint
import hashlib
import keyword
import tomllib
from copy import deepcopy
from pathlib import Path

TARGETS=frozenset({'client/python/zipzi/_generated/catalog.py','client/python/zipzi/docs/reference.md'})
PUBLIC_SPECIALS=frozenset({'__enter__','__exit__','__repr__'})

def compact(source):
    lines=source.splitlines();result=[];decorated=False;changed=False
    for line in lines:
        if line.startswith('@dataclass('):
            if 'repr=' not in line:line=line[:-1]+', repr=False)'
            decorated=True
        elif decorated and line.startswith('class '):
            if line.endswith(':') and '(' not in line:line=line[:-1]+'(CompactValue):'
            elif '(Generic[' in line:line=line.replace('(Generic[','(CompactValue, Generic[')
            decorated=False;changed=True
        elif line and not line.startswith('@') and not line.startswith('class '):decorated=False
        result.append(line)
    if changed:
        index=next((i+1 for i,x in enumerate(result) if x.startswith('from __future__ import')),1)
        result.insert(index,'from .._presentation import CompactValue')
    return '\n'.join(result)+'\n'


def render_catalog(contract,output,header,package,dto_names,modeling_name):
    parsed = {}
    def parse(path, content):
        cached = parsed.get(path)
        if cached is None or cached[0] != content:
            cached = parsed[path] = (content, ast.parse(content))
        return cached[1]
    # Read sources rather than importing them: generation never creates Runtime,
    # imports native code, introspects private tokens or depends on an installed wheel.
    sources={p.relative_to(package).as_posix():p.read_text(encoding='utf-8') for p in (package/'client/python/zipzi').rglob('*.py') if '__pycache__' not in p.parts}
    sources.update({p:s for p,s in output.items() if p.endswith('.py')})
    for p,s in list(output.items()):
        if p.endswith('.py') and '@dataclass(' in s:output[p]=compact(s);sources[p]=output[p]
    for path,content in list(output.items()):
        if path.endswith('/_generated/modeling.py'):
            tree=parse(path, content)
            names=[n.name for n in tree.body if isinstance(n,ast.ClassDef) and not n.name.startswith('_')]
            names += [n.target.id for n in tree.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and 'TypeAlias' in ast.unparse(n.annotation)]
            names += ['ExtensionValue']
            output[path]=content+'\n__all__ = '+repr(names)+'\n'
            sources[path]=output[path]
    read_classes=set()
    for path,content in sources.items():
        if '/_generated/' not in path and not path.endswith('/_values.py'):continue
        for node in parse(path, content).body:
            if isinstance(node,ast.ClassDef) and any(isinstance(f,ast.FunctionDef) and f.name in {'read','_read'} for f in node.body):read_classes.add(node.name)
    pattern=r'\b('+'|'.join(sorted(read_classes))+r')\.read\b'
    for path,content in list(output.items()):
        if not path.endswith('.py'):continue
        content=re.sub(pattern,r'\1._read',content)
        content=content.replace('    def read(', '    def _read(').replace('    def wire(', '    def _wire(').replace('.wire()', '._wire()')
        output[path]=content;sources[path]=content
    shapes={}
    def visit(value):
        if isinstance(value,dict):
            if isinstance(value.get('kind'),str) and value['kind'] in {'record','serde_record','union','serde_enum'}:shapes[value['name']]=value
            for item in value.values():visit(item)
        elif isinstance(value,list):
            for item in value:visit(item)
    visit(contract)
    class_shapes={}
    def bind(file,name,shape):class_shapes[(file,name)]=shape
    def variants(shape,file,prefix=''):
        for variant in shape.get('variants',[]):
            fields=variant['fields']
            if file=='resource_ops':
                # Keep the reviewed ImageCreateSpec flattening used by resource_ops.
                fields=[item for f in fields for item in (f['shape']['fields'] if f['name']=='spec' else [f])]
            bind(file,prefix+variant['rust_name'],dict(name=shape['name']+'::'+variant['rust_name'],fields=fields))
    for section,file in ((contract,'meshes'),(contract['resources'],'resource_values'),(contract['scene'],'scene')):
        for binding in section['bindings']:
            if binding['form']=='external':continue
            shape=shapes.get(binding['rust'])
            if shape is None:continue
            if binding['form']=='union':
                destination='mesh_ops' if file=='meshes' else 'resource_ops' if file=='resource_values' else file
                variants(shape,destination)
            else:
                destination='plan_values' if binding['name'] in {'PlanCost','PlanReport'} else file
                bind(destination,binding['name'],shape)
    for rust,shape in shapes.items():
        if rust in {s['name'] for s in contract['modeling']['roots']} or rust.startswith(('zipzi_contract::','zipzi_core::','zipzi_sdk::','zipzi_runtime::','zipzi_animation::','zipzi_project::')):
            short=modeling_name(rust)
            if shape['kind'] in {'union','serde_enum'}:variants(shape,'modeling',short)
            else:bind('modeling',short,shape)
        for domain,file in [('parameters','parameter_values'),('bindings','binding_values'),('sampling','sampling_values'),('references','reference_values')]:
            name=dto_names[domain].get(rust.rsplit('::',1)[-1])
            if name is None:continue
            if shape['kind'] in {'union','serde_enum'}:variants(shape,file)
            else:bind(file,name,shape)
    for name,shape in contract['runtime'].items():bind('discovery',name,shape)
    def compiled_fields(file,node):
        shape=class_shapes.get((file,node.name))
        if shape is None or shape.get('kind') == 'serde_enum':return []
        selected={n.target.id for n in node.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and not n.target.id.startswith('_')}
        fields=[dict(f,name=f['name']+('_' if keyword.iskeyword(f['name']) else ''))
                for f in shape['fields'] if f['name']+('_' if keyword.iskeyword(f['name']) else '') in selected and not f.get('skip')]
        if (file,node.name)==('plan_values','PlanReport'):
            # The bound Plan wrapper adds its captured Runtime source to the SDK report.
            snapshot=shapes['zipzi_contract::runtime::PlanSnapshot']
            fields=[next(f for f in snapshot['fields'] if f['name']=='source')]+fields
        if 'kind' in selected and not any(f['name']=='kind' for f in fields):
            fields=fields+[dict(name='kind',doc='Fixed operation discriminator supplied by this value type; not a constructor argument or a mutable object field.')]
        missing=[f['name'] for f in fields if not f.get('doc','').strip()]
        if missing:
            raise ValueError('Missing semantic field documentation: '+shape['name']+'.'+', '.join(missing))
        return [(f['name'],f['doc']) for f in fields]
    def python_fields(node,selected=None):
        declared=[n.target.id for n in node.body if isinstance(n,ast.AnnAssign)
                  and isinstance(n.target,ast.Name) and not n.target.id.startswith('_')
                  and not (isinstance(n.annotation,ast.Subscript)
                           and ast.unparse(n.annotation.value) in {'ClassVar','_ClassVar'})]
        if selected is not None:declared=[name for name in declared if name in selected]
        if not declared:return []
        text=ast.get_docstring(node) or ''
        field_text=text.partition('Fields:\n')[2]
        documented={}
        current=None
        for line in field_text.splitlines():
            match=re.fullmatch(r'([A-Za-z_]\w*):\s*(.+)',line)
            if match:
                current=match[1];documented[current]=match[2]
            elif current and line.strip():documented[current]+=' '+line.strip()
        missing=[name for name in declared if name not in documented]
        if missing:
            raise ValueError('Missing semantic field documentation: '+node.name+'.'+', '.join(missing))
        return [(name,documented[name]) for name in declared]
    aliases={'MeshSpec':'MeshCreateSpec','Geometry':'MeshGeometry','ParameterSpec':'ParameterCreateSpec','MeshBindingSpec':'MeshBindingCreateSpec','Checks':'GeometryChecks'}
    for path,content in list(output.items()):
        if not path.endswith('.py') or '/_generated/' not in path or '@dataclass' not in content:continue
        tree=parse(path, content);lines=content.splitlines(True);edits=[]
        for node in tree.body:
            if not isinstance(node,ast.ClassDef) or ast.get_docstring(node) or not any(isinstance(n,ast.Name) and n.id=='CompactValue' for n in node.bases):continue
            fs=compiled_fields(Path(path).stem,node)
            text='Immutable owned '+node.name+' value. Construction owns declared collections; reading fields, repr and summary performs no IO.'
            if fs:text+='\n\nFields:\n'+'\n'.join(name+': '+doc for name,doc in fs)
            edits.append((node.body[0].lineno-1,'    '+repr(text)+'\n'))
        for index,value in reversed(edits):lines.insert(index,value)
        output[path]=''.join(lines);sources[path]=output[path]
    prefix='client/python/zipzi/'
    trees={p[len(prefix):-3].replace('/','.') if p!=prefix+'__init__.py' else '':parse(p, s) for p,s in sources.items() if p.startswith(prefix)}
    symbols={}
    def signature(node):
        if isinstance(node,ast.FunctionDef):
            arguments=deepcopy(node.args)
            if arguments.args and arguments.args[0].arg in {'self','cls'}:arguments.args.pop(0)
            return '('+ast.unparse(arguments)+')'+(' -> '+ast.unparse(node.returns) if node.returns else '')
        init=next((n for n in node.body if isinstance(n,ast.FunctionDef) and n.name=='__init__'),None)
        if init:
            if any(a.arg in {'backend','token','session','edit'} for a in init.args.args[1:]) and node.name in {'Runtime','Session','Edit','Job','Plan','PreparedImage','ImageLease','Buffer','Preview','ObjectRef','LabelAddress','FieldAddress'}:return '(factory-owned; use the documented public entry)'
            return signature(init)
        fields=[n for n in node.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and not n.target.id.startswith('_') and not (isinstance(n.annotation,ast.Subscript) and ast.unparse(n.annotation.value) in {'ClassVar','_ClassVar'})]
        shape=class_shapes.get((node_modules.get(id(node),'').rsplit('.',1)[-1],node.name),{})
        declared={f['name']+('_' if keyword.iskeyword(f['name']) else ''):f for f in shape.get('fields',[])}
        def default(field):
            if field.value is None:return ''
            source=declared.get(field.target.id,{})
            value=source.get('default')
            kind=source.get('shape',{}).get('kind')
            # Render public values from the compiled contract, never evaluate
            # generated factories or change their runtime implementation.
            if value is not None:
                if kind=='sequence' and value==[]:return ' = ()'
                if kind in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'} and not isinstance(value,dict):
                    return ' = '+repr(value)
            return ' = '+ast.unparse(field.value)
        return '('+', '.join(n.target.id+': '+ast.unparse(n.annotation)+default(n) for n in fields)+')'
    for module,tree in trees.items():
        table={}
        for node in tree.body:
            if isinstance(node,(ast.ClassDef,ast.FunctionDef)):table[node.name]=node
            elif isinstance(node,ast.ImportFrom) and node.level:
                parent=module.split('.')[:-1] if module else []
                if module.endswith('.__init__'):parent=module.split('.')[:-1]
                target='.'.join(parent[:len(parent)-node.level+1]+(node.module or '').split('.')).strip('.')
                for a in node.names:table[a.asname or a.name]=(target,a.name)
            elif isinstance(node,(ast.Assign,ast.AnnAssign)):
                for n in getattr(node,'targets',[getattr(node,'target',None)]):
                    if isinstance(n,ast.Name):table[n.id]=node
        symbols[module]=table
    def resolve(module,name,seen=None):
        seen=set() if seen is None else seen
        if (module,name) in seen:return None
        seen.add((module,name));node=symbols.get(module,{}).get(name)
        if isinstance(node,tuple):return resolve(*node,seen)
        if isinstance(node,ast.AnnAssign) and 'TypeAlias' in ast.unparse(node.annotation) and isinstance(node.value,ast.Name):
            # A single-variant union is the actual dataclass at runtime. Its
            # alias needs the same field docs and inherited public methods.
            return resolve(module,node.value.id,seen)
        return node
    node_modules={id(node):module for module,tree in trees.items() for node in tree.body if isinstance(node,ast.ClassDef)}
    def public_methods(node,seen=None):
        seen=set() if seen is None else seen
        if id(node) in seen:return {}
        seen.add(id(node));methods={}
        module=node_modules.get(id(node),'')
        for base in node.bases:
            value=base.value if isinstance(base,ast.Subscript) else base
            parent=resolve(module,value.id) if isinstance(value,ast.Name) else None
            if isinstance(parent,ast.ClassDef):methods.update(public_methods(parent,seen))
        methods.update({n.name:n for n in node.body if isinstance(n,ast.FunctionDef)
                        and (not n.name.startswith('_') or n.name in PUBLIC_SPECIALS)})
        return methods
    catalog={'zipzi':dict(kind='module',signature='',documentation='Typed authoring API. Start with docs/guide.md and docs/recipes.py. Use local() or connect(), explicitly select sessions, read before editing, verify confirmed results, then save and release.',example='docs/recipes.py',children=[])}
    def add(path,node,field_docs=None):
        kind='type' if isinstance(node,ast.ClassDef) else 'callable' if isinstance(node,ast.FunctionDef) else 'value'
        text=ast.get_docstring(node) if isinstance(node,(ast.ClassDef,ast.FunctionDef)) else ''
        if kind=='callable' and not text:
            raise ValueError('Missing semantic callable documentation: '+path)
        fields=field_docs or []
        if isinstance(node,ast.ClassDef):
            fields=fields or compiled_fields(node_modules.get(id(node),'').rsplit('.',1)[-1],node)
            if not fields:fields=python_fields(node)
            else:
                selected={n.target.id for n in node.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name)
                          and not n.target.id.startswith('_') and not (isinstance(n.annotation,ast.Subscript)
                          and ast.unparse(n.annotation.value) in {'ClassVar','_ClassVar'})}
                extra=selected-set(dict(fields))
                if extra:fields+=python_fields(node,extra)
        catalog[path]=dict(kind=kind,signature=signature(node) if kind in {'type','callable'} else '',documentation=text or ('Immutable typed value. See field semantics and the linked workflow.' if kind=='type' else 'Typed public API entry.'),fields=fields,children=[],example='docs/recipes.py')
        if isinstance(node,ast.ClassDef):
            for child in public_methods(node).values():
                add(path+'.'+child.name,child);catalog[path]['children'].append(path+'.'+child.name)
    # Public paths are explicit reexports in domain modules, plus canonical modeling DTO types/unions.
    domains=('meshes','parameters','bindings','resources','scene','sampling','events','references','modeling','ops','animation','spatial','observation','inspection')
    for module in ('',*domains):
        public='zipzi'+('.'+module if module else '')
        if module:catalog[public]=dict(kind='module',signature='',documentation=ast.get_docstring(trees[module]) or '',children=[])
        names=[]
        if module=='':
            for node in trees[''].body:
                if isinstance(node,(ast.Assign,ast.AugAssign)) and any(isinstance(t,ast.Name) and t.id=='__all__' for t in (node.targets if isinstance(node,ast.Assign) else [node.target])):
                    try:names+=ast.literal_eval(node.value)
                    except (ValueError,TypeError):pass
        elif any(isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='__all__' for t in n.targets) for n in trees[module].body):
            for n in trees[module].body:
                if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='__all__' for t in n.targets):names+=ast.literal_eval(n.value)
        elif module=='modeling':
            names=['ExtensionValue']
            names+=[n.name for n in trees['_generated.modeling'].body if isinstance(n,ast.ClassDef) and not n.name.startswith('_')]
            names += [n.target.id for n in trees['_generated.modeling'].body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and 'TypeAlias' in ast.unparse(n.annotation)]
        else:
            for name,node in symbols[module].items():
                if name.startswith('_') or name in {'Backend','Source','Snapshot','TYPE_CHECKING','Callable','Generic','Self','TypeVar','Plan','Owned','RecoveryOwner','Mapping','Sequence','record','sequence','string','inputs','read','wire','validate','batch','check_limit','OBJECT_KINDS','OPERATION_TYPES','Inputs'}:continue
                if isinstance(node,tuple) and node[0].startswith('_generated') or isinstance(node,(ast.ClassDef,ast.FunctionDef)) or module=='ops':names.append(name)
        for name in dict.fromkeys(names):
            if name in domains:continue
            node=resolve(module,name)
            if node is None and module=='modeling':node=resolve('_generated.modeling',name)
            if node is None:continue
            add(public+'.'+name,node);catalog[public]['children'].append(public+'.'+name)
    # Register bound facade paths as users encounter them on Session/Edit.
    for owner in ('Session','Edit'):
        for cls,domain in [('Meshes','meshes'),('Parameters','parameters'),('Bindings','bindings'),('Images','resources'),('Scene','scene'),('Modeling','modeling')]:
            base=resolve('_generated.facades',cls)
            if base is None:continue
            path='zipzi.'+owner+'.'+domain;add(path,base)
            if owner=='Edit':
                edit=resolve('_generated.facades','Editable'+cls)
                if edit:
                    for child in edit.body:
                        if isinstance(child,ast.FunctionDef) and not child.name.startswith('_'):add(path+'.'+child.name,child);catalog[path]['children'].append(path+'.'+child.name)
            if 'zipzi.'+owner in catalog:catalog['zipzi.'+owner]['children'].append(path)
    for owner,module,cls in [('Runtime.sessions','_lifecycle','Sessions'),('Session.project','_lifecycle','Project'),('Runtime.resources','resources','Resources')]:
        node=resolve(module,cls)
        if node:add('zipzi.'+owner,node)
    # Derive navigation from registered paths, including bound properties and
    # domain modules, so direct lookup and parent traversal cannot diverge.
    for path in catalog:
        parent=path.rpartition('.')[0]
        if parent in catalog:catalog[parent]['children'].append(path)
    fingerprint=hashlib.sha256()
    for path,source in sorted(sources.items()):
        if not path.startswith(prefix) or path.endswith('/catalog.py'):continue
        fingerprint.update(path.encode());fingerprint.update(b'\0');fingerprint.update(source.encode())
    guide_path=prefix+'docs/guide.md'
    fingerprint.update(guide_path.encode());fingerprint.update(b'\0')
    fingerprint.update((package/guide_path).read_text(encoding='utf-8').encode())
    source_sha=fingerprint.hexdigest()
    package_version=tomllib.loads((package/'client/pyproject.toml').read_text(encoding='utf-8'))['project']['version']
    catalog['zipzi']['documentation']+=' Package '+package_version+'; protocol '+contract['protocol']+'; client source SHA-256 '+source_sha+'.'
    for row in catalog.values():row['children']=tuple(sorted(set(row.get('children',()))))
    reference=[header.rstrip(),'# Installed Python API reference','','Generated from the compiled authoring contract, the finite binding directory and lifecycle source docs.','']
    for path,row in sorted(catalog.items()):
        reference+=['## '+path,'', '`'+row.get('signature','')+'`','',row.get('documentation',''),'']
        for field,text in row.get('fields',()):reference+=['- `'+field+'`: '+text]
        reference+=['']
    result={'client/python/zipzi/_generated/catalog.py':header+'"""Offline API directory; no imports or host probes."""\nfrom typing import TypedDict, NotRequired\nfrom collections.abc import Sequence\nclass _Entry(TypedDict):\n    kind: str\n    signature: str\n    documentation: str\n    fields: NotRequired[Sequence[tuple[str,str]]]\n    children: tuple[str,...]\n    example: NotRequired[str]\nCONTRACT = '+repr(contract['protocol'])+'\nPACKAGE_VERSION = '+repr(package_version)+'\nCONTRACT_SHA256 = '+repr(hashlib.sha256(__import__('json').dumps(contract,sort_keys=True).encode()).hexdigest())+'\nCATALOG: dict[str,_Entry] = '+pprint.pformat(catalog,width=120,sort_dicts=True)+'\n', 'client/python/zipzi/docs/reference.md':'\n'.join(reference)+'\n'}
    result['client/python/zipzi/_generated/catalog.py']=result['client/python/zipzi/_generated/catalog.py'].replace(
        'CONTRACT_SHA256 = ', 'CLIENT_SOURCE_SHA256 = '+repr(source_sha)+'\nCONTRACT_SHA256 = ',1)
    return result
