"""Prepare the generated Python API and its static documentation inputs."""
import ast
import importlib.util
import tomllib
from copy import deepcopy
from dataclasses import replace
from codegen.model import DocEntry, DocRule, Projection, collection_rules, validate_rules, validate_projections
import keyword
import re
from pathlib import Path
from codegen.projections import READ_CLASSES, SPECIAL_PROJECTIONS

_fingerprint_spec=importlib.util.spec_from_file_location('zipzi_source_fingerprint', Path(__file__).parents[2]/'client/python/zipzi/_fingerprint.py')
_fingerprint=importlib.util.module_from_spec(_fingerprint_spec)
_fingerprint_spec.loader.exec_module(_fingerprint)
PUBLIC_SPECIALS=frozenset({'__enter__','__exit__','__repr__'})

# Bound only to variant projections with an emitted init=False Literal field.
DISCRIMINATOR_RULE = DocRule('fixed-discriminator', 'Fixed operation discriminators',
    'A variant constructor supplies its fixed operation discriminator automatically. '
    'The attribute is read-only and is not a constructor argument or an editable business field. '
    'Choose a variant by its documented business effect; the discriminator identifies its wire operation.',
    'generator:codegen.python_api.compiled_fields')

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


def build_api(contract,package,dto_names,modeling_name,resolve=lambda shape: shape,graph=None):
    parsed = {}
    def parse(path, content):
        cached = parsed.get(path)
        if cached is None or cached[0] != content:
            cached = parsed[path] = (content, ast.parse(content))
        return cached[1]
    shapes={}
    def visit(value):
        if isinstance(value,dict):
            value=resolve(value)
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
                fields=[item for f in fields for item in (resolve(f['shape'])['fields'] if f['name']=='spec' else [f])]
            bind(file,prefix+variant['rust_name'],dict(name=shape['name']+'::'+variant['rust_name'],
                 rust_type_id=shape['name'],variant=variant['rust_name'],doc=variant.get('doc',''),fields=fields))
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
            bind('modeling',short,shape)
            if shape['kind'] in {'union','serde_enum'}:variants(shape,'modeling',short)
        for domain,file in [('parameters','parameter_values'),('bindings','binding_values'),('sampling','sampling_values'),('references','reference_values')]:
            name=dto_names[domain].get(rust.rsplit('::',1)[-1])
            if name is None:continue
            if shape['kind'] in {'union','serde_enum'}:variants(shape,file)
            else:bind(file,name,shape)
    for name,shape in contract['runtime'].items():bind('discovery',name,resolve(shape))
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
        for name, role, value in field_roles(file,node):
            fields=fields+[dict(name=name,doc='Fixed value '+value+' (read-only).')]
        missing=[f['name'] for f in fields if not f.get('doc','').strip()]
        if missing:
            raise ValueError('Missing semantic field documentation: '+shape['name']+'.'+', '.join(missing))
        return [(f['name'],f['doc']) for f in fields]
    def field_roles(file,node):
        shape=class_shapes.get((file,node.name),{})
        if not shape.get('variant'):return ()
        roles=[]
        declared={f['name'] for f in shape.get('fields',())}
        for field in node.body:
            if not isinstance(field,ast.AnnAssign) or not isinstance(field.target,ast.Name) or field.target.id in declared:continue
            if (isinstance(field.annotation,ast.Subscript) and ast.unparse(field.annotation.value)=='Literal'
                    and isinstance(field.value,ast.Call) and ast.unparse(field.value.func)=='field'
                    and any(arg.arg=='init' and isinstance(arg.value,ast.Constant) and arg.value.value is False for arg in field.value.keywords)):
                value=next(arg.value for arg in field.value.keywords if arg.arg=='default')
                if ast.literal_eval(value)!=ast.literal_eval(field.annotation.slice):
                    raise ValueError('Projection discriminator mismatch: '+shape['name'])
                roles.append((field.target.id,'fixed-discriminator',ast.unparse(value)))
            elif (isinstance(field.annotation,ast.Subscript) and ast.unparse(field.annotation.value)=='ClassVar'
                  and isinstance(field.annotation.slice,ast.Subscript) and ast.unparse(field.annotation.slice.value)=='Literal'
                  and isinstance(field.value,ast.Constant)):
                if ast.literal_eval(field.value)!=ast.literal_eval(field.annotation.slice.slice):
                    raise ValueError('Projection discriminator mismatch: '+shape['name'])
                roles.append((field.target.id,'fixed-discriminator',ast.unparse(field.value)))
        return tuple(roles)
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
    pattern=r'\b('+'|'.join(sorted(READ_CLASSES))+r')\.read\b'
    projected=[]
    def render_python(path,content):
        if not path.endswith('.py'):return content
        if '/_generated/' in path:
            tree=parse(path,content)
            readers={node.name for node in tree.body if isinstance(node,ast.ClassDef)
                     and any(isinstance(f,ast.FunctionDef) and f.name in {'read','_read'} for f in node.body)}
            missing=readers-READ_CLASSES
            if missing:raise ValueError('Unreviewed value readers: '+', '.join(sorted(missing)))
            if graph is not None:
                file=Path(path).stem
                for node in tree.body:
                    if isinstance(node,ast.ClassDef):name=node.name
                    elif (file=='modeling' and isinstance(node,ast.AnnAssign)
                          and isinstance(node.target,ast.Name) and 'TypeAlias' in ast.unparse(node.annotation)):
                        name=node.target.id
                    else:continue
                    shape=class_shapes.get((file,name))
                    if shape is None:continue
                    identity=shape.get('rust_type_id',shape['name'])
                    if identity not in graph.domains:continue
                    domain=graph.domains[identity]
                    if file=='modeling':
                        variant=shape.get('variant')
                        projected.append(Projection(identity,'canonical' if variant is None else 'variant:'+variant,
                                                    domain,'zipzi._generated.values.'+domain,
                                                    'zipzi.modeling.'+domain+'.'+name,
                                                    reason=None if variant is None else 'Typed constructor for the '+variant+' variant of '+identity+'.'))
                    else:
                        reason=SPECIAL_PROJECTIONS.get(file)
                        if reason is None:raise ValueError('Unreviewed special projection: '+file+'.'+node.name)
                        module=next((n.value.value for n in node.body if isinstance(n,ast.Assign)
                                     and len(n.targets)==1 and isinstance(n.targets[0],ast.Name)
                                     and n.targets[0].id=='__module__' and isinstance(n.value,ast.Constant)
                                     and isinstance(n.value.value,str)), 'zipzi._generated.'+file)
                        if module.startswith('zipzi._generated.'):
                            continue
                        projected.append(Projection(identity,file+'.'+name,domain,'zipzi._generated.'+file,
                                                    module+'.'+name,
                                                    construction='convenience',field_conversion='owned-client',
                                                    wire='compiled-shape',reason=reason))
        if '@dataclass(' in content:content=compact(content)
        if path.endswith('/_generated/modeling.py'):
            tree=parse(path,content)
            names=[n.name for n in tree.body if isinstance(n,ast.ClassDef) and not n.name.startswith('_')]
            names += [n.target.id for n in tree.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and 'TypeAlias' in ast.unparse(n.annotation)]
            names += ['ExtensionValue']
            content=content+'\n__all__ = '+repr(names)+'\n'
        content=re.sub(pattern,r'\1._read',content)
        content=content.replace('    def read(', '    def _read(').replace('    def wire(', '    def _wire(').replace('.wire()', '._wire()')
        if '/_generated/' not in path:return content
        tree=parse(path,content);lines=content.splitlines(True);edits=[]
        for node in tree.body:
            if not isinstance(node,ast.ClassDef) or not any(isinstance(n,ast.Name) and n.id in {'CompactValue','NamedTuple','Enum'} for n in node.bases):continue
            roles=field_roles(Path(path).stem,node)
            if ast.get_docstring(node) and not roles:continue
            fs=compiled_fields(Path(path).stem,node)
            shape=class_shapes.get((Path(path).stem,node.name),{})
            text=shape.get('doc','').strip()
            if fs:text+='\n\nFields:\n'+'\n'.join(name+': '+doc for name,doc in fs)
            if roles:text+='\n\n'+DISCRIMINATOR_RULE.documentation
            first=node.body[0]
            edits.append((first.lineno-1,first.end_lineno if ast.get_docstring(node) else first.lineno-1,'    '+repr(text)+'\n'))
        for start,end,value in reversed(edits):lines[start:end]=[value]
        return ''.join(lines)
    def prepare(output):
        # Handwritten source docs may be read, but generated outputs always come
        # from this render, never from files left in the checkout.
        sources={}
        prefix=package/'client/python/zipzi'
        for path in prefix.rglob('*.py'):
            relative=path.relative_to(package).as_posix()
            if '__pycache__' in path.parts or '_generated' in path.parts or relative in output:continue
            content=path.read_text(encoding='utf-8')
            if content.startswith('# @generated by zipzi-python/tools/generate.py; DO NOT EDIT.\n'):continue
            sources[relative]=content
        sources.update({path:content for path,content in output.items() if path.endswith('.py')})
        for path,content in output.items():
            if '/_generated/values/' not in path or not path.endswith('.py') or path.endswith('/__init__.py'):continue
            domain=Path(path).stem
            for node in parse(path,content).body:
                name=node.name if isinstance(node,ast.ClassDef) else node.target.id if isinstance(node,ast.AnnAssign) and isinstance(node.target,ast.Name) else None
                if name is not None and ('modeling',name) in class_shapes:
                    class_shapes[(domain,name)]=class_shapes[('modeling',name)]
        validate_projections(tuple(projected))
        return sources,parse,class_shapes,compiled_fields,python_fields,tuple(projected),resolve,field_roles
    return render_python,prepare


def document_model(contract,prepared,package,bindings=None):
    sources,parse,class_shapes,compiled_fields,python_fields,projected,resolve_shape,field_roles=prepared
    bindings={} if bindings is None else bindings
    rules=collection_rules(package)
    rules[DISCRIMINATOR_RULE.key]=DISCRIMINATOR_RULE
    prefix='client/python/zipzi/'
    def module_name(path):
        suffix=path[len(prefix):]
        if suffix=='__init__.py':return ''
        if suffix.endswith('/__init__.py'):suffix=suffix[:-len('/__init__.py')]
        else:suffix=suffix[:-3]
        return suffix.replace('/','.')
    packages={module_name(p) for p in sources if p.endswith('/__init__.py')}
    trees={module_name(p):parse(p,s) for p,s in sources.items() if p.startswith(prefix)}
    symbols={}
    definition_docs={}
    identities={}
    def identify(module,nodes,parents=()):
        for node in nodes:
            name=(node.name if isinstance(node,(ast.ClassDef,ast.FunctionDef)) else
                  node.target.id if isinstance(node,ast.AnnAssign) and isinstance(node.target,ast.Name) else
                  node.targets[0].id if isinstance(node,ast.Assign) and isinstance(node.targets[0],ast.Name) else None)
            if name is not None:
                identities[id(node)]='python:'+'.'.join(('zipzi', *((module,) if module else ()), *parents, name))
                if isinstance(node,ast.ClassDef):identify(module,node.body,(*parents,name))
    for module,tree in trees.items():identify(module,tree.body)
    def signature(node):
        if isinstance(node,ast.FunctionDef):
            if any(isinstance(d,ast.Name) and d.id=='property' for d in node.decorator_list):
                return ': '+(ast.unparse(node.returns) if node.returns else 'object')
            arguments=deepcopy(node.args)
            if arguments.args and arguments.args[0].arg in {'self','cls'}:arguments.args.pop(0)
            return '('+ast.unparse(arguments)+')'+(' -> '+ast.unparse(node.returns) if node.returns else '')
        init=next((n for n in node.body if isinstance(n,ast.FunctionDef) and n.name=='__init__'),None)
        if init:
            if any(a.arg in {'backend','token','session','edit'} for a in init.args.args[1:]) and node.name in {'Runtime','Session','Edit','Job','Plan','PreparedImage','ImageLease','Buffer','Preview','ObjectRef','LabelAddress','FieldAddress','Resources','SessionSubscription','RegistrySubscription'}:return '(factory-owned; use the documented public entry)'
            return signature(init)
        fields=[n for n in node.body if isinstance(n,ast.AnnAssign) and isinstance(n.target,ast.Name) and not n.target.id.startswith('_') and not (isinstance(n.annotation,ast.Subscript) and ast.unparse(n.annotation.value) in {'ClassVar','_ClassVar'})]
        fields=[n for n in fields if not (isinstance(n.value,ast.Call) and isinstance(n.value.func,ast.Name) and n.value.func.id=='field' and any(a.arg=='init' and isinstance(a.value,ast.Constant) and a.value.value is False for a in n.value.keywords))]
        shape=class_shapes.get((node_modules.get(id(node),'').rsplit('.',1)[-1],node.name),{})
        declared={f['name']+('_' if keyword.iskeyword(f['name']) else ''):f for f in shape.get('fields',[])}
        def default(field):
            if field.value is None:return ''
            source=declared.get(field.target.id,{})
            value=source.get('default')
            shape=resolve_shape(source.get('shape',{}))
            kind=shape.get('kind')
            # Render public values from the compiled contract, never evaluate
            # generated factories or change their runtime implementation.
            if value is not None:
                if kind=='sequence' and value==[]:return ' = ()'
                if kind in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'} and not isinstance(value,dict):
                    return ' = '+repr(value)
                if kind in {'union','serde_enum'} and all(not v['fields'] for v in shape['variants']):
                    variant=next((v for v in shape['variants'] if v.get('name',v.get('wire_name'))==value),None)
                    if variant is not None:
                        member=re.sub(r'(?<!^)(?=[A-Z])','_',variant['rust_name']).upper()
                        return ' = '+ast.unparse(field.annotation)+'.'+member
            if isinstance(field.value,ast.Call) and isinstance(field.value.func,ast.Name) and field.value.func.id=='field':
                explicit=next((a.value for a in field.value.keywords if a.arg=='default'),None)
                return ' = '+ast.unparse(explicit) if explicit is not None else ' = <factory>'
            return ' = '+ast.unparse(field.value)
        kw_only=any(isinstance(decorator,ast.Call) and isinstance(decorator.func,ast.Name)
                    and decorator.func.id=='dataclass' and any(arg.arg=='kw_only' and ast.literal_eval(arg.value) is True for arg in decorator.keywords)
                    for decorator in node.decorator_list)
        positional=[];keywords=[]
        for n in fields:
            field_kw=kw_only
            if isinstance(n.value,ast.Call) and isinstance(n.value.func,ast.Name) and n.value.func.id=='field':
                flag=next((a.value for a in n.value.keywords if a.arg=='kw_only'),None)
                if isinstance(flag,ast.Constant):field_kw=flag.value
            (keywords if field_kw else positional).append(n.target.id+': '+ast.unparse(n.annotation)+default(n))
        return '('+', '.join(positional+(['*']+keywords if keywords else []))+')'
    def type_scope(nodes):
        for node in nodes:
            yield node
            if isinstance(node,ast.If) and ast.unparse(node.test) in {'TYPE_CHECKING','typing.TYPE_CHECKING'}:
                yield from type_scope(node.body)
    for module,tree in trees.items():
        for left,right in zip(tree.body,tree.body[1:]):
            if isinstance(left,(ast.Assign,ast.AnnAssign)) and isinstance(right,ast.Expr) and isinstance(right.value,ast.Constant) and isinstance(right.value.value,str):
                definition_docs[id(left)]=right.value.value
        table={}
        for node in type_scope(tree.body):
            if isinstance(node,(ast.ClassDef,ast.FunctionDef)):table[node.name]=node
            elif isinstance(node,ast.ImportFrom) and (node.level or node.module=='zipzi' or (node.module or '').startswith('zipzi.')):
                parent=module.split('.') if module in packages and module else module.split('.')[:-1] if module else []
                target='.'.join(parent[:len(parent)-node.level+1]+(node.module or '').split('.')).strip('.')
                if not node.level:target=(node.module or '').removeprefix('zipzi').lstrip('.')
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
    node_modules={id(node):module for module,tree in trees.items() for node in tree.body if isinstance(node,(ast.ClassDef,ast.AnnAssign,ast.Assign))}
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
    catalog={'zipzi':dict(kind='module',signature='',documentation='Typed authoring API. Start with docs/guide.md and docs/recipes.py. Use local() or connect(), explicitly select sessions, read before editing, verify confirmed results, then save and release.',children=[])}
    def add(path,node,field_docs=None):
        kind='type' if isinstance(node,ast.ClassDef) else 'callable' if isinstance(node,ast.FunctionDef) else 'value'
        text=ast.get_docstring(node) if isinstance(node,(ast.ClassDef,ast.FunctionDef)) else definition_docs.get(id(node),'')
        if kind=='callable' and not text:
            raise ValueError('Missing semantic callable documentation: '+path)
        if isinstance(node,ast.FunctionDef) and any(isinstance(d,ast.Name) and d.id=='property' for d in node.decorator_list):kind='property'
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
        identity=identities[id(node)]
        binding=bindings.get(identity)
        roles=field_roles(node_modules.get(id(node),'').rsplit('.',1)[-1],node) if isinstance(node,ast.ClassDef) else ()
        # Compiled summaries and descriptors are acquired before rendering; no
        # consumer trims expanded shared prose out of a docstring.
        if roles:
            shape=class_shapes[(node_modules[id(node)].rsplit('.',1)[-1],node.name)]
            text=shape.get('doc','')
        catalog[path]=dict(kind=kind,signature=signature(node) if kind in {'type','callable'} else '',documentation=binding.summary if binding else text or '',fields=fields,children=[],_identity=identity,
                           _node=id(node),example=binding.example if binding else None,rule_refs=binding.rule_refs if binding else ('fixed-discriminator',) if roles else (),field_roles=roles)
        if (isinstance(node,ast.AnnAssign) and 'TypeAlias' in ast.unparse(node.annotation)) or (isinstance(node,ast.Assign) and isinstance(node.value,(ast.Subscript,ast.BinOp))):
            name=node.target.id if isinstance(node,ast.AnnAssign) else node.targets[0].id
            shape=class_shapes.get((node_modules.get(id(node),'').rsplit('.',1)[-1],name),{})
            catalog[path].update(kind='alias',signature=ast.unparse(node.value),documentation=text or shape.get('doc') or '')
        if isinstance(node,ast.ClassDef) and any(isinstance(b,ast.Name) and b.id=='Enum' for b in node.bases):
            kind='enum'
            shape=class_shapes.get((node_modules.get(id(node),'').rsplit('.',1)[-1],node.name),{})
            variant_docs={variant.get('wire_name',variant.get('name')):variant.get('doc','').strip() for variant in shape.get('variants',())}
            member_docs={id(left):right.value.value.strip() for left,right in zip(node.body,node.body[1:])
                         if isinstance(left,ast.Assign) and isinstance(right,ast.Expr)
                         and isinstance(right.value,ast.Constant) and isinstance(right.value.value,str)}
            members=[]
            for member in node.body:
                if not isinstance(member,ast.Assign) or not isinstance(member.targets[0],ast.Name) or member.targets[0].id.startswith('_'):continue
                meaning=variant_docs.get(ast.literal_eval(member.value),'') or member_docs.get(id(member),'')
                members.append((member.targets[0].id,ast.unparse(member.value)+(' — '+meaning if meaning else '')))
            catalog[path].update(kind='enum',signature='',fields=members,documentation=text or '')
            catalog[path]['_diagnostics']=[('enum-member',name,'review_required' if variant_docs.get(ast.literal_eval(member.value),'') or member_docs.get(id(member),'') else 'missing')
                for member in node.body if isinstance(member,ast.Assign) and isinstance(member.targets[0],ast.Name)
                and not (name:=member.targets[0].id).startswith('_')]
        if isinstance(node,(ast.ClassDef,ast.Assign,ast.AnnAssign)):
            module=node_modules.get(id(node),'')
            name=node.name if isinstance(node,ast.ClassDef) else node.target.id if isinstance(node,ast.AnnAssign) else node.targets[0].id
            shape=class_shapes.get((module.rsplit('.',1)[-1],name),{})
            catalog[path]['_documentation_source']=('rust:'+shape['name'] if shape else identity)
            if shape.get('variants') and kind!='enum':
                catalog[path]['_diagnostics']=[('union-branch',v['rust_name'],'review_required' if v.get('doc','').strip() else 'missing') for v in shape['variants']]
            elif shape.get('variant'):
                catalog[path]['_diagnostics']=[('union-branch',shape['variant'],'review_required' if shape.get('doc','').strip() else 'missing')]
        if isinstance(node,ast.FunctionDef):
            parameters=[arg.arg for arg in (*node.args.posonlyargs,*node.args.args,*node.args.kwonlyargs) if arg.arg not in {'self','cls'}]
            catalog[path]['_diagnostics']=[('callable-parameters',','.join(parameters),'review_required' if parameters else 'not_applicable'),
                ('callable-return',ast.unparse(node.returns) if node.returns else '', 'review_required' if node.returns else 'missing')]
        if kind=='property':catalog[path]['signature']=signature(node)
        if isinstance(node,ast.ClassDef):
            for child in public_methods(node).values():
                add(path+'.'+child.name,child);catalog[path]['children'].append(path+'.'+child.name)
    # Public paths are explicit reexports in domain modules, plus canonical modeling DTO types/unions.
    # Root exports determine public modules, including handwritten optional APIs.
    root_exports=[]
    for node in trees[''].body:
        if isinstance(node,(ast.Assign,ast.AugAssign)) and any(isinstance(t,ast.Name) and t.id=='__all__' for t in (node.targets if isinstance(node,ast.Assign) else [node.target])):
            root_exports.extend(ast.literal_eval(node.value))
    domains=tuple(name for name in root_exports if name in trees)
    modeling_domains=tuple(sorted(name.removeprefix('modeling.') for name in trees if name.startswith('modeling.') and '.' not in name[len('modeling.'):]))
    for module in ('',*domains,*('modeling.'+name for name in modeling_domains)):
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
            names=[]
        else:
            for name,node in symbols[module].items():
                if name.startswith('_') or name in {'Backend','Source','Snapshot','TYPE_CHECKING','Callable','Generic','Self','TypeVar','Plan','Owned','RecoveryOwner','Mapping','Sequence','record','sequence','string','inputs','read','wire','validate','batch','check_limit','OBJECT_KINDS','OPERATION_TYPES','Inputs'}:continue
                if isinstance(node,tuple) and node[0].startswith('_generated') or isinstance(node,(ast.ClassDef,ast.FunctionDef)) or module=='ops':names.append(name)
        for name in dict.fromkeys(names):
            if name in domains or module=='modeling' and name in modeling_domains:continue
            node=resolve(module,name)
            if node is None:continue
            add(public+'.'+name,node);catalog[public]['children'].append(public+'.'+name)
    # Register bound facade paths as users encounter them on Session/Edit.
    for owner in ('Session','Edit'):
        for cls,domain in [('Meshes','meshes'),('Parameters','parameters'),('Bindings','bindings'),('Images','resources'),('Scene','scene'),('Modeling','modeling')]:
            base=resolve('_generated.facades',cls)
            if base is None:continue
            path='zipzi.'+owner+'.'+domain;add(path,base)
            catalog[path].update(kind='property',signature=owner.lower()+'.'+domain,documentation=ast.get_docstring(base) or catalog[path]['documentation'])
            if owner=='Edit':
                edit=resolve('_generated.facades','Editable'+cls)
                if edit:
                    catalog[path]['documentation']=ast.get_docstring(edit) or catalog[path]['documentation']
                    for child in edit.body:
                        if isinstance(child,ast.FunctionDef) and not child.name.startswith('_'):add(path+'.'+child.name,child);catalog[path]['children'].append(path+'.'+child.name)
            if 'zipzi.'+owner in catalog:catalog['zipzi.'+owner]['children'].append(path)
            for child_path in catalog[path]['children']:
                row=catalog[child_path]
                if any(ref in {'collection-read','collection-page'} for ref in row.get('rule_refs',())):
                    context=('Reads committed Session content.' if owner=='Session' else 'Reads this Edit candidate, including pending writes.')
                    row['documentation']=context+'\n\n'+row['documentation']
    for owner,module,cls in [('Runtime.sessions','_lifecycle','Sessions'),('Session.project','_lifecycle','Project'),('Runtime.resources','resources','Resources')]:
        node=resolve(module,cls)
        if node:
            add('zipzi.'+owner,node)
            catalog['zipzi.'+owner].update(kind='property',signature=owner[0].lower()+owner[1:])
    # Derive navigation from registered paths, including bound properties and
    # domain modules, so direct lookup and parent traversal cannot diverge.
    for path in catalog:
        parent=path.rpartition('.')[0]
        if parent in catalog:catalog[parent]['children'].append(path)
    for path in (package/prefix/'docs').rglob('*.md'):
        relative=path.relative_to(package).as_posix()
        if _fingerprint.included(relative):sources[relative]=path.read_text(encoding='utf-8')
    for path,row in catalog.items():
        example=row.get('example')
        if example and not (package/prefix/example).is_file():
            raise ValueError('Missing API example: '+example)
    unused=set(bindings)-{row.get('_identity') for row in catalog.values()}
    if unused:raise ValueError('Unbound documentation projection: '+', '.join(sorted(unused)))
    source_sha=_fingerprint.fingerprint(sources)
    package_version=tomllib.loads((package/'client/pyproject.toml').read_text(encoding='utf-8'))['project']['version']
    compact_class=resolve('_presentation','CompactValue')
    common={'Owned values':ast.get_docstring(compact_class)}
    common.update({n.name:ast.get_docstring(n) for n in compact_class.body
            if isinstance(n,ast.FunctionDef) and n.name in {'summary','__repr__'}})
    # Resolve annotation names through source imports and public object identities.
    # This runs before presentation: Markdown and offline help share the same refs.
    public_types={}
    canonical_paths={projection.canonical_path for projection in projected}
    for path,row in sorted(catalog.items(),key=lambda item:(item[0] not in canonical_paths,item[0].count('.'),item[0])):
        if row['kind'] in {'type','enum','alias'}:
            public_types.setdefault(row.get('_node'),path)
    definitions={id(node):(module,node) for module,tree in trees.items() for node in ast.walk(tree)
                 if isinstance(node,(ast.ClassDef,ast.FunctionDef,ast.Assign,ast.AnnAssign))}
    imported_modules={}
    external_names={}
    for module,tree in trees.items():
        aliases={}
        external=set()
        for node in type_scope(tree.body):
            if isinstance(node,ast.Import):
                for alias in node.names:
                    target=alias.name.removeprefix('zipzi.')
                    aliases[alias.asname or alias.name.split('.')[0]]=target if alias.asname else alias.name.split('.')[0]
                    if not alias.name.startswith('zipzi'):external.add(alias.asname or alias.name.split('.')[0])
            elif isinstance(node,ast.ImportFrom):
                if node.level:
                    parent=module.split('.') if module in packages else module.split('.')[:-1]
                    target='.'.join(parent[:len(parent)-node.level+1]+(node.module or '').split('.')).strip('.')
                elif node.module=='zipzi' or (node.module or '').startswith('zipzi.'):
                    target=(node.module or '').removeprefix('zipzi').lstrip('.')
                else:
                    external.update(alias.asname or alias.name for alias in node.names)
                    continue
                for alias in node.names:
                    candidate=(target+'.'+alias.name).strip('.')
                    if candidate in trees:aliases[alias.asname or alias.name]=candidate
        imported_modules[module]=aliases
        external_names[module]=external
    reference_diagnostics=set()
    def annotation_refs(module,annotation,aliases=frozenset()):
        if annotation is None:return set()
        if isinstance(annotation,ast.Constant) and isinstance(annotation.value,str):
            try:return annotation_refs(module,ast.parse(annotation.value,mode='eval').body,aliases)
            except SyntaxError:
                reference_diagnostics.add(('type-reference',annotation.value,'unresolved-sdk'))
                return set()
        if isinstance(annotation,(ast.Name,ast.Attribute)):
            parts=ast.unparse(annotation).split('.')
            node=None
            if len(parts)==1:node=resolve(module,parts[0])
            else:
                target=imported_modules.get(module,{}).get(parts[0])
                if target is not None:node=resolve('.'.join([target,*parts[1:-1]]),parts[-1])
                elif parts[0]=='zipzi':node=resolve('.'.join(parts[1:-1]),parts[-1])
            path=public_types.get(id(node))
            name=ast.unparse(annotation)
            variable=resolve(module,parts[0])
            if path:
                reference_diagnostics.add(('type-reference',name,'public-sdk'))
                return {path}
            # A private alias can describe a public union without being itself an
            # exported API. Follow its definition, retaining cycle protection.
            if isinstance(node,(ast.Assign,ast.AnnAssign)) and isinstance(node.value,(ast.BinOp,ast.Subscript)) and id(node) not in aliases:
                return annotation_refs(node_modules[id(node)],node.value,aliases | {id(node)})
            if parts[0] in {'str','int','float','bool','bytes','bytearray','memoryview','object','tuple','list','dict','set','frozenset','type','range','Exception','BaseException'}:
                classification='builtin'
            elif parts[0] in {'typing','Any','Self','Generic','Sequence','Mapping','Callable','Iterator','Iterable','Literal','ClassVar','TypeAlias','Protocol','Never','Optional','Union','TypeVar'}:
                classification='typing'
            elif isinstance(variable,(ast.Assign,ast.AnnAssign)) and isinstance(variable.value,ast.Call) and ast.unparse(variable.value.func) in {'TypeVar','typing.TypeVar','ParamSpec','typing.ParamSpec'}:
                classification='type-variable'
            elif parts[0] in external_names.get(module,set()):classification='external'
            else:classification='unresolved-sdk'
            reference_diagnostics.add(('type-reference',name,classification))
            return set()
        if isinstance(annotation,ast.Subscript) and ast.unparse(annotation.value).split('.')[-1]=='Literal':return set()
        return set().union(*(annotation_refs(module,child,aliases) for child in ast.iter_child_nodes(annotation)))
    for path,row in catalog.items():
        reference_diagnostics.clear()
        definition=definitions.get(row.get('_node'))
        if definition is None:continue
        module,node=definition
        annotations=[]
        if isinstance(node,ast.ClassDef):
            init=next((child for child in node.body if isinstance(child,ast.FunctionDef) and child.name=='__init__'),None)
            if init and row['kind'] != 'property' and not row['signature'].startswith('(factory-owned'):node=init
            else:annotations=[child.annotation for child in node.body if isinstance(child,ast.AnnAssign)]
        if isinstance(node,ast.FunctionDef):
            annotations=[arg.annotation for arg in [*node.args.posonlyargs,*node.args.args,*node.args.kwonlyargs]]+[node.returns]
            annotations += [arg.annotation for arg in (node.args.vararg,node.args.kwarg) if arg is not None]
        elif isinstance(node,(ast.Assign,ast.AnnAssign)):
            annotations=[node.value]
        refs=set().union(*(annotation_refs(module,annotation) for annotation in annotations))
        row['type_refs']=tuple(sorted(refs-{path}))
        row.setdefault('_diagnostics',[]).extend(sorted(reference_diagnostics))
    # Projection identity distinguishes convenience/canonical constructors even
    # when their Rust source is the same. Reexports share the defining identity.
    projection_ids={catalog[p.canonical_path]['_identity']:'rust:'+p.rust_type_id+'|projection:'+p.projection_id
                    for p in projected if p.canonical_path in catalog}
    for path,row in catalog.items():
        if row.get('_identity') in projection_ids:row['_identity']=projection_ids[row['_identity']]
        validate_rules(rules,tuple(row.get('rule_refs',())))
        for ref in row.get('type_refs',()):
            if ref not in catalog:raise ValueError('Dangling public type reference: '+path+' -> '+ref)
    entries=[DocEntry.from_row(path,row,common) for path,row in sorted(catalog.items())]
    by_identity={}
    by_path={entry.path:entry for entry in entries}
    for entry in entries:
        if entry.identity is not None and entry.kind in {'type','value','enum','alias'}:
            by_identity.setdefault(entry.identity,[]).append(entry.path)
    resolved=[]
    canonical_paths={projection.canonical_path for projection in projected}
    for projection in projected:
        entry=by_path.get(projection.canonical_path)
        if entry is None:
            raise ValueError('Missing public projection: '+projection.canonical_path)
        aliases=() if entry is None or entry.identity is None else tuple(
            path for path in by_identity[entry.identity] if path != projection.canonical_path and path not in canonical_paths)
        resolved.append(replace(projection,aliases=aliases))
    validate_projections(tuple(resolved))
    return entries,domains,common,package_version,source_sha,prefix,rules,tuple(resolved)
