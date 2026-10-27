"""Executable installed-package workflows. Run with python -I /path/to/recipes.py.

Use local by default; --endpoint selects a running Connected host. --folder must
be an absolute directory accessible to that host. No source import path is added.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
import argparse
import os
import zipzi
from zipzi import meshes,parameters,bindings,scene


def author_and_save(runtime:zipzi.Runtime,folder:Path)->zipzi.SaveReceipt:
    session=runtime.sessions.create()
    geometry=meshes.Geometry((42,7,99),((0.,0.),(1.,0.),(0.,1.)),((0.,0.),(1.,0.),(0.,1.)),((42,7,99),))
    with session.edit('agent recipe') as edit:
        mesh=zipzi.recover_call(edit.meshes.create,meshes.MeshSpec('Draft',geometry))
        parameter=edit.parameters.create(parameters.ParameterSpec('Turn',-1.,1.,0.))
        edit.apply_many((meshes.Rename(mesh,'Face'),parameters.Patch(parameter,parameters.ParameterPatch(default_value=.25))))
        edit.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(parameter,(-1.,1.)),),(bindings.Keyform((-1.,),geometry.positions),bindings.Keyform((1.,),tuple((x+10.,y) for x,y in geometry.positions)))))
        assert edit.meshes.get(mesh).value.geometry.vertex_ids==(42,7,99)
        assert edit.parameters.get(parameter).value.default_value==.25
    assert edit.receipt is not None and edit.receipt.changed
    page=session.meshes.list(name='Face')
    assert len(page.items)==1 and page.items[0].id==mesh and page.next_cursor is None
    assert session.modeling.sample(({parameter:0.},)).value.frames[0].drawables[0].positions[0]==(5.,0.)
    with session.plan(meshes.Rename(mesh,'Final',expected_name='Face')) as plan:
        with session.edit('planned rename') as edit:edit.apply(plan)
    assert session.meshes.get(mesh).value.name=='Final'
    with runtime.resources.prepare_image(1,1,bytes((255,7,9,255))) as image:
        with session.edit('image') as edit:asset=edit.resources.create_image('Pixel',image)
    assert session.resources.get(asset).value.pixels.rgba==bytes((255,7,9,255))
    saved=session.project.save(folder/'project').wait()
    # Windows Local publication cannot synchronize directories. Connected
    # receipts describe the host, so keep their durability check intact.
    assert saved.durable or (os.name == "nt" and runtime.connection is None)
    session.close()
    reopened=runtime.sessions.open(saved.manifest).wait()
    assert reopened.resources.get(asset).value.pixels.rgba==bytes((255,7,9,255))
    reopened.close()
    return saved


def close_repeatedly(runtime:zipzi.Runtime)->None:
    for _ in range(40):runtime.sessions.create().close(discard_modified=True)


def read_all(session:zipzi.Session)->tuple[str,...]:
    result: list[str]=[];cursor=None;source=None
    while True:
        page=session.meshes.list(cursor=cursor)
        if source is None:source=page.source
        assert page.source==source
        result.extend(item.id for item in page.items)
        cursor=page.next_cursor
        if cursor is None:return tuple(result)


def discover_owned(session:zipzi.Session,part_id:str,name:str)->zipzi.Snapshot[meshes.Mesh]:
    """Resolve one exact-name mesh by Part ownership, preserving one discovery Source.

    Ambiguity raises ValueError. A concurrent publication propagates STALE_SOURCE;
    callers can explicitly restart rather than mixing versions or guessing IDs.
    """
    matches: list[str]=[];cursor=None;source=None
    while True:
        page=session.meshes.list(name=name,limit=1,cursor=cursor)
        if source is None:source=page.source
        assert page.source==source
        matches.extend(item.id for item in page.items if item.part_id==part_id)
        cursor=page.next_cursor
        if cursor is None:break
    if len(matches)!=1:raise ValueError('Expected one mesh in the specified Part')
    return session.meshes.get(matches[0],expected=source)


def recover_public_calls(session:zipzi.Session)->str:
    """Recover creation, commit Job and terminal wait through registered public calls.

    Each helper calls once and attempts the original recovery once; continued
    uncertainty propagates OutcomeUnknown to the caller, never a guessed success.
    """
    geometry=meshes.Geometry((42,7,99),((0.,0.),(1.,0.),(0.,1.)),((0.,0.),(1.,0.),(0.,1.)),((42,7,99),))
    with session.edit('recover original calls') as edit:
        identity=zipzi.recover_call(edit.meshes.create,meshes.MeshSpec('Recovered',geometry))
        assert isinstance(identity,str)
        job=zipzi.recover_call(edit.commit)
        assert isinstance(job,zipzi.Job)
        receipt=zipzi.recover_call(job.wait)
        assert receipt.changed
    assert edit.receipt==receipt
    assert session.meshes.get(identity).value.name=='Recovered'
    assert session.info().history==(1,0)
    return identity


def discovery_and_failure(runtime:zipzi.Runtime)->None:
    """Select a duplicate name by ownership; discard a failed Edit and recreate it."""
    session=runtime.sessions.create()
    geometry=meshes.Geometry((42,7,99),((0.,0.),(1.,0.),(0.,1.)),((0.,0.),(1.,0.),(0.,1.)),((42,7,99),))
    with session.edit('duplicate names') as edit:
        part=edit.apply(scene.CreatePart(scene.PartSpec('Target part'))).ids[0]
        target=edit.meshes.create(meshes.MeshSpec('Repeated',geometry))
        other=edit.meshes.create(meshes.MeshSpec('Repeated',geometry))
        edit.apply(scene.MoveToPart((target,),part))
    found=discover_owned(session,part,'Repeated')
    assert found.value.id==target and set(read_all(session))=={target,other}
    before=session.info()
    failed=session.edit('ordinary failure')
    try:
        with failed:
            failed.meshes.rename(target,'Wrong',expected_name='Stale name')
    except zipzi.SdkFailure:
        assert failed.state==zipzi.EditState.aborted
    else:raise AssertionError('Expected a confirmed stale-label failure')
    assert session.info()==before
    with session.edit('fresh candidate') as edit:
        edit.meshes.rename(target,'Correct',expected_name='Repeated')
    assert session.meshes.get(target).value.name=='Correct'
    assert session.meshes.get(other).value.name=='Repeated'
    session.close(discard_modified=True)


def main()->None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint',type=Path)
    parser.add_argument('--folder',type=Path)
    args=parser.parse_args()
    if args.endpoint and args.folder is None:parser.error('--folder is required for Connected host paths')
    runtime=zipzi.local() if args.endpoint is None else zipzi.connect(endpoint=args.endpoint)
    with runtime,TemporaryDirectory() as temporary:
        folder=args.folder or Path(temporary).resolve()
        if not folder.is_absolute():parser.error('--folder must be absolute on the host')
        print(author_and_save(runtime,folder).summary())
        close_repeatedly(runtime)

if __name__=='__main__':main()
