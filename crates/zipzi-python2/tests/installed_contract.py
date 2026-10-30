"""D01/D02/D04/D07 offline audit against only the normal installed wheel."""
from pathlib import Path
from dataclasses import is_dataclass,fields
import argparse
import importlib.util
import inspect
import typing
import re
from urllib.parse import unquote
from importlib import metadata,resources
import zipzi
from zipzi._generated.catalog import lookup,iter_paths
from zipzi._generated import facades
from zipzi._lifecycle import Sessions,Project
from zipzi.resources import Resources


def markdown_anchors(text:str)->set[str]:
    """Resolve Markdown heading anchors, including repeated headings and HTML ids."""
    anchors=set(re.findall(r'<[\w-]+\b[^>]*\bid=["\']([^"\']+)["\']',text))
    counts:dict[str,int]={}
    for heading in re.findall(r'^#{1,6}\s+(.+?)\s*#*\s*$',text,re.MULTILINE):
        heading=re.sub(r'\[([^]]+)\]\([^)]+\)',r'\1',heading).lower()
        base=re.sub(r'\s+','-',re.sub(r'[^\w\-\s]','',heading)).strip('-')
        count=counts.get(base,0);counts[base]=count+1
        anchors.add(base if count==0 else base+'-'+str(count))
    return anchors


def markdown_link_failures(document:Path)->list[str]:
    failures=[]
    for link in re.findall(r'(?<!!)\[[^\]]+\]\(([^)]+)\)',document.read_text(encoding='utf-8')):
        if '://' in link or link.startswith('mailto:'):continue
        destination,separator,fragment=link.partition('#')
        target=document.parent/unquote(destination) if destination else document
        if not target.is_file():failures.append('invalid installed link '+link)
        elif separator and unquote(fragment) not in markdown_anchors(target.read_text(encoding='utf-8')):
            failures.append('invalid installed anchor '+link)
    return failures


def audit()->dict[str,object]:
    paths=set(iter_paths())
    failures:dict[str,list[str]]={key:[] for key in ('D01','D02','D04','D07','D08')}
    counts={'symbols':0,'callables':0,'fields':0}
    if '/site-packages/zipzi/' not in Path(zipzi.__file__).as_posix():raise RuntimeError('Audit requires an installed wheel')
    if importlib.util.find_spec('zipzi_native') is not None:raise RuntimeError('Offline audit must use the pure wheel only')
    modules=[zipzi]
    for module in modules:
        modules.extend(value for name in getattr(module,'__all__',())
                       if inspect.ismodule(value:=getattr(module,name,None))
                       and value.__name__.startswith('zipzi.') and value not in modules)
    def visit(path:str,value:object,constructor:bool=True)->None:
        counts['symbols']+=1
        if path not in paths:failures['D01'].append(path+': missing from installed catalog')
        if inspect.isclass(value):
            try:inspect.signature(value);typing.get_type_hints(value)
            except Exception as error:failures['D04'].append(path+': '+str(error))
            if is_dataclass(value) or issubclass(value,tuple) and hasattr(value,'_fields'):
                documented=dict(lookup(path).get('fields',())) if path in paths else {}
                field_names=[field.name for field in fields(value)] if is_dataclass(value) else value._fields
                text=inspect.getdoc(value) or ''
                for name in field_names:
                    if name.startswith('_'):continue
                    counts['fields']+=1
                    description=documented.get(name,'').strip()
                    if not description or description.startswith('Owned ') and description.endswith(' value.'):
                        failures['D02'].append(path+'.'+name+': missing semantic field description')
                    elif ' '.join((name+': '+description).split()) not in ' '.join(text.split()):
                        failures['D02'].append(path+'.'+name+': missing field docstring')
                if 'Declared by ' in text:failures['D02'].append(path+': Rust fields still have declaration-only descriptions')
            for name in dir(value):
                if name.startswith('_') and name not in {'__enter__','__exit__','__repr__'}:continue
                member=inspect.getattr_static(value,name)
                if name.startswith('_'):
                    source=inspect.getsourcefile(member) if inspect.isfunction(member) else None
                    if source is None or not Path(source).is_relative_to(Path(zipzi.__file__).parent):continue
                if isinstance(member,property):
                    if member.fget is not None:check_callable(path+'.'+name,member.fget)
                elif inspect.isfunction(member):check_callable(path+'.'+name,member)
        elif inspect.isfunction(value):check_callable(path,value)
    def check_callable(path:str,value:object)->None:
        counts['callables']+=1
        if path not in paths:failures['D01'].append(path+': missing callable catalog entry')
        try:
            inspect.signature(value);typing.get_type_hints(value)
        except Exception as error:failures['D04'].append(path+': '+str(error))
        if not inspect.getdoc(value):failures['D02'].append(path+': missing semantic docstring')
    for module in modules:
        names=getattr(module,'__all__',None)
        if names is None:
            failures['D01'].append(module.__name__+': missing explicit __all__')
            continue
        for name in names:
            try:value=getattr(module,name)
            except AttributeError:failures['D01'].append(module.__name__+'.'+name+': invalid export');continue
            if not inspect.ismodule(value):visit(module.__name__+'.'+name,value)
    for owner in ('Session','Edit'):
        for cls,domain in ((facades.Meshes,'meshes'),(facades.Parameters,'parameters'),(facades.Bindings,'bindings'),(facades.Images,'resources'),(facades.Scene,'scene'),(facades.Modeling,'modeling')):
            if owner=='Edit':cls=getattr(facades,'Editable'+cls.__name__,cls)
            visit('zipzi.'+owner+'.'+domain,cls,False)
    for path,cls in (('zipzi.Runtime.sessions',Sessions),('zipzi.Runtime.resources',Resources),('zipzi.Session.project',Project)):visit(path,cls,False)
    source=zipzi.Source('79900000-0000-4000-8000-000000000001',1,zipzi.Version(1,0,0))
    positions=tuple((float(n),0.) for n in range(1000))
    geometry=zipzi.meshes.Geometry(tuple(range(1000)),positions,positions,())
    frame=zipzi.sampling.Frame(0,(),(),(),(),())
    sampled=zipzi.sampling.Result('formal',source.version,(),(frame,),(),())
    values=(frame,sampled,geometry,zipzi.resources.ImagePixels(1024,1024,bytes(4<<20)),zipzi.Snapshot(source,geometry),zipzi.Batch(source,tuple(range(1000000))),zipzi.Page(source,(geometry,)))
    for value in values:
        if len(repr(value))>1024:failures['D07'].append(type(value).__name__+': repr exceeds 1024 chars')
        if value.summary()!=repr(value):failures['D07'].append(type(value).__name__+': summary differs')
    if len(geometry.positions)!=1000:failures['D07'].append('Explicit geometry fields changed')
    from zipzi._generated.metadata import PACKAGE_VERSION,CONTRACT,CONTRACT_SHA256,CLIENT_SOURCE_SHA256
    if PACKAGE_VERSION!=metadata.version('zipzi'):failures['D08'].append('Installed package/catalog version mismatch')
    if CONTRACT!='zipzi-runtime-v2' or not re.fullmatch('[0-9a-f]{64}',CONTRACT_SHA256):failures['D08'].append('Missing protocol major or contract provenance')
    package=resources.files('zipzi')
    from zipzi._fingerprint import fingerprint,included
    sources={}
    package_path=Path(zipzi.__file__).parent
    for path in package_path.rglob('*'):
        if not path.is_file():continue
        relative='client/python/zipzi/'+path.relative_to(package_path).as_posix()
        if included(relative):sources[relative]=path.read_text(encoding='utf-8')
    if fingerprint(sources)!=CLIENT_SOURCE_SHA256:failures['D08'].append('Installed source/document build fingerprint mismatch')
    if CLIENT_SOURCE_SHA256 not in zipzi.describe('zipzi').documentation:failures['D08'].append('Build fingerprint missing from public root help')
    for reference in {'docs/guide.md','docs/reference.md','docs/recipes.py',*[lookup(path).get('example','') for path in paths]}-{''}:
        if not package.joinpath(reference).is_file():failures['D08'].append('Missing installed reference: '+reference)
    guide=package.joinpath('docs/guide.md').read_text()
    if len(guide.split())>1500:failures['D08'].append('Short workflow exceeds 1500 words')
    for document in (package_path/'docs').rglob('*.md'):
        name=document.relative_to(package_path/'docs').as_posix()
        failures['D08'].extend(name+': '+failure for failure in markdown_link_failures(document))
    return {'counts':counts,'failures':{k:sorted(set(v)) for k,v in failures.items()}}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    result=audit();lines=['# Installed offline contract audit','',str(result['counts']),'']
    failed=False
    for key,failures in result['failures'].items():
        lines+=[f'{key}: '+('failed' if failures else 'passed')]
        for failure in failures:lines+=['- '+failure]
        failed|=bool(failures)
    args.output.write_text('\n'.join(lines)+'\n');print(str(result['counts']))
    for key,failures in result['failures'].items():print(key,len(failures))
    raise SystemExit(1 if failed else 0)

if __name__=='__main__':main()
