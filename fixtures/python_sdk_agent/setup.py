"""Build independent fixture documents through a normal installed zipzi package.

Invoked by the external controller, never copied into the agent's readable packet.
Local tasks receive saved projects; Connected tasks receive resident Session IDs.
"""
from pathlib import Path
from uuid import UUID,uuid5
import json
import sys
import zipzi
from zipzi import meshes,scene


def prepare(task_path:Path,controller_path:Path)->None:
    data=json.loads(task_path.read_text());private=json.loads(controller_path.read_text())
    endpoint=data['endpoint'];folder=Path(data['host_folder'])
    def uid(label:str)->str:return str(uuid5(UUID(data['document_id']),label))
    with (zipzi.local() if endpoint is None else zipzi.connect(endpoint=Path(endpoint))) as runtime:
        manifests=[];identities=[]
        for index in range(2 if private['task']=='G01' else 1):
            session=runtime.sessions.create(data['document_id'] if index==0 else uid('other-document'))
            geometry=meshes.Geometry(tuple(data['vertex_ids']),data['positions'],data['uvs'],data['triangles'])
            count=int(data['objects']) if private['task'] in {'G01','G03','G04'} else 0
            for start in range(0,count,64):
                with session.edit('fixture objects') as edit:
                    for n in range(start,min(start+64,count)):
                        identity=data['mesh_id'] if n==0 else private['hidden']['other_ids'][n-1] if n<3 else uid(f'mesh-{n}')
                        edit.meshes.create(meshes.MeshSpec('Face',geometry,id=identity))
            if count:
                with session.edit('fixture ownership') as edit:
                    edit.apply(scene.CreatePart(scene.PartSpec(data['part_name'],id=uid('target-part'))))
                    edit.apply(scene.MoveToPart((data['mesh_id'],),uid('target-part')))
            saved=session.project.save(folder/f'fixture-{index}').wait()
            manifests.append(str(saved.manifest));identities.append({'runtime':session.id.runtime,'serial':session.id.serial})
            if endpoint is None:session.close()
        data['fixture_projects']=manifests
        if endpoint is not None:data['fixture_sessions']=identities
    if private['task'] in {'G01','G03'}:data.pop('mesh_id')
    task_path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')

if __name__=='__main__':prepare(Path(sys.argv[1]),Path(sys.argv[2]))
