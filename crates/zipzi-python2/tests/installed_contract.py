"""D01/D02/D04/D07 offline audit against only the normal installed wheel."""
from pathlib import Path
from dataclasses import is_dataclass,fields
import argparse
import importlib.util
import inspect
import typing
import re
import hashlib
from importlib import metadata,resources
import zipzi
from zipzi._generated.catalog import CATALOG
from zipzi._generated import facades
from zipzi._lifecycle import Sessions,Project
from zipzi.resources import Resources


def audit()->dict[str,object]:
    failures:dict[str,list[str]]={key:[] for key in ('D01','D02','D04','D07','D08')}
    counts={'symbols':0,'callables':0,'fields':0}
    if '/site-packages/zipzi/' not in zipzi.__file__:raise RuntimeError('Audit requires an installed wheel')
    if importlib.util.find_spec('zipzi_native') is not None:raise RuntimeError('Offline audit must use the pure wheel only')
    modules=[zipzi,*[getattr(zipzi,n) for n in ('meshes','parameters','bindings','resources','scene','sampling','events','references','modeling','ops','animation','spatial','observation','inspection')]]
    def visit(path:str,value:object,constructor:bool=True)->None:
        counts['symbols']+=1
        if path not in CATALOG:failures['D01'].append(path+': missing from installed catalog')
        if inspect.isclass(value):
            try:inspect.signature(value);typing.get_type_hints(value)
            except Exception as error:failures['D04'].append(path+': '+str(error))
            if is_dataclass(value):
                documented=dict(CATALOG.get(path,{}).get('fields',()))
                for field in fields(value):
                    if field.name.startswith('_'):continue
                    counts['fields']+=1
                    description=documented.get(field.name,'').strip()
                    if not description or description.startswith('Owned ') and description.endswith(' value.'):
                        failures['D02'].append(path+'.'+field.name+': missing semantic field description')
                text=inspect.getdoc(value) or ''
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
        if path not in CATALOG:failures['D01'].append(path+': missing callable catalog entry')
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
    sampled=zipzi.sampling.Result('formal',source.version,(),(frame,),())
    values=(frame,sampled,geometry,zipzi.resources.ImagePixels(1024,1024,bytes(4<<20)),zipzi.Snapshot(source,geometry),zipzi.Batch(source,tuple(range(1000000))),zipzi.Page(source,(geometry,)))
    for value in values:
        if len(repr(value))>1024:failures['D07'].append(type(value).__name__+': repr exceeds 1024 chars')
        if value.summary()!=repr(value):failures['D07'].append(type(value).__name__+': summary differs')
    if len(geometry.positions)!=1000:failures['D07'].append('Explicit geometry fields changed')
    from zipzi._generated.catalog import PACKAGE_VERSION,CONTRACT,CONTRACT_SHA256,CLIENT_SOURCE_SHA256
    if PACKAGE_VERSION!=metadata.version('zipzi'):failures['D08'].append('Installed package/catalog version mismatch')
    if CONTRACT!='zipzi-runtime-v2' or not re.fullmatch('[0-9a-f]{64}',CONTRACT_SHA256):failures['D08'].append('Missing protocol major or contract provenance')
    package=resources.files('zipzi')
    fingerprint=hashlib.sha256()
    package_path=Path(zipzi.__file__).parent
    for path in sorted(package_path.rglob('*.py')):
        if path.relative_to(package_path).as_posix()=='_generated/catalog.py':continue
        relative='client/python/zipzi/'+path.relative_to(package_path).as_posix()
        fingerprint.update(relative.encode());fingerprint.update(b'\0');fingerprint.update(path.read_text().encode())
    fingerprint.update(b'client/python/zipzi/docs/guide.md\0')
    fingerprint.update(package.joinpath('docs/guide.md').read_text().encode())
    if fingerprint.hexdigest()!=CLIENT_SOURCE_SHA256:failures['D08'].append('Installed source/document build fingerprint mismatch')
    if CLIENT_SOURCE_SHA256 not in zipzi.describe('zipzi').documentation:failures['D08'].append('Build fingerprint missing from public root help')
    for reference in {'docs/guide.md','docs/reference.md','docs/recipes.py',*[entry.get('example','') for entry in CATALOG.values()]}-{''}:
        if not package.joinpath(reference).is_file():failures['D08'].append('Missing installed reference: '+reference)
    guide=package.joinpath('docs/guide.md').read_text()
    if len(guide.split())>1500:failures['D08'].append('Short workflow exceeds 1500 words')
    for name in ('guide.md','reference.md'):
        document=package.joinpath('docs',name)
        for link in re.findall(r'(?<!!)\[[^\]]+\]\(([^)]+)\)',document.read_text()):
            if '://' in link or link.startswith('#'):continue
            if not package.joinpath('docs',link.split('#',1)[0]).is_file():failures['D08'].append(name+': invalid installed link '+link)
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
