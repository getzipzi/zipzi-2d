"""Fixed, reproducible agent task inputs. Hidden expectations stay outside packets."""
from dataclasses import dataclass
from uuid import UUID, uuid5

FIXTURE_VERSION='p3a-agent-fixture-3-observed'
VERIFIER_VERSION='p3a-independent-verifier-4-public-runtime'
SEEDS={f'G{index:02}':tuple(314159+index*100+repeat for repeat in range(3)) for index in range(1,9)}

@dataclass(frozen=True)
class Task:
    id:str
    environment:str
    seed:int
    data:dict[str,object]
    hidden:dict[str,object]


def task(task_id:str,environment:str,repeat:int)->Task:
    if task_id not in SEEDS or environment not in ('local','connected') or not 0<=repeat<3:
        raise ValueError('Unknown fixed task/environment/repetition')
    if task_id in ('G07','G08') and environment!='connected':raise ValueError('Recovery fault tasks require Connected')
    seed=SEEDS[task_id][repeat]
    def uid(kind:str)->str:return str(uuid5(UUID('fb5ed6e9-6ad2-40f1-8773-0529f44d9a57'),f'{seed}:{kind}'))
    data={'document_id':uid('document'),'name':'Face','part_name':'Target part','final_name':f'Completed {seed}',
          'vertex_ids':[42,7,99],'positions':[[0.,0.],[1.,0.],[0.,1.]],'uvs':[[0.,0.],[1.,0.],[0.,1.]],'triangles':[[42,7,99]],
          'parameter_id':uid('parameter'),'mesh_id':uid('target'),'binding_id':uid('binding'),
          'pose':.5,'rgba_initial':[255,7,9,255],'rgba_final':[7,255,9,255],
          'objects':257 if task_id=='G03' else 3,'session_iterations':40 if task_id=='G06' else 1}
    # Complete domain inputs before the first model run; these specify desired
    # results, never an SDK implementation or hidden verifier answer.
    data.update(parameter_name='Turn',parameter_range=[0.,1.],parameter_default=0.,
                axis_keys=[0.,1.],keyform_positions=[data['positions'],[[15.,0.],[16.,0.],[15.,1.]]],
                image_width=1,image_height=1,image_name='Pixel',
                output_folder_name='result',
                output_report='output/result.json')
    data['failure_expected_name']='Intentionally incorrect expected name'
    data['requirements']={
        'G01':'In the specified document rename only Face owned by Target part to final_name; other objects and the other document stay unchanged. Save to host_folder/result.',
        'G02':'Create exactly the supplied mesh, parameter and one mesh binding using the supplied IDs/range/axis keys/keyform positions. Sample pose before and after publication. Save to host_folder/result.',
        'G03':'Enumerate exactly objects meshes at one consistent Source, recovering from the external page-time publication. Rename only Face owned by Target part to final_name. Save to host_folder/result.',
        'G04':'Plan renaming mesh_id to final_name against formal state, explain cost/losses, handle the external stale Source. Exercise one ordinary rename failure using failure_expected_name as the expected name, then rebuild a fresh Edit and complete exactly one intended publication. Save to host_folder/result.',
        'G05':'Create a one-pixel image from rgba_initial, replace its pixels with rgba_final keeping its identity; Undo must restore initial pixels and Redo final pixels. Save, close, reopen and verify. Save to host_folder/result.',
        'G06':'Start with the fixture registry. Create 40 additional sessions in sequence, edit one mesh per session, exercise unsaved-close refusal, explicitly discard 39 sessions, save the last to host_folder/result and close it. Reopen and verify the saved mesh, close it, and confirm registry returns to its initial contents.',
        'G07':'Create exactly one mesh using mesh_id and the supplied geometry; rename it to final_name within the same Edit, recovering both lost replies without replaying either write. Save to host_folder/result.',
        'G08':'Create exactly one mesh with mesh_id, geometry and final_name; recover the lost commit and terminal Job reply without a second publication. Save to host_folder/result and inspect durable receipt.'
    }[task_id]
    hidden={'target_id':uid('target'),'other_ids':[uid('other-0'),uid('other-1')],
            'expected_sample_x':7.5,'expected_pixels':[7,255,9,255],
            'required_controller_events':{'G03':['source_changed_between_pages'],'G04':['plan_source_changed','ordinary_write_failed'],'G07':['create_reply_dropped','apply_reply_dropped'],'G08':['commit_reply_dropped','terminal_reply_dropped']}.get(task_id,[])}
    return Task(task_id,environment,seed,data,hidden)


def matrix():
    return tuple(task(f'G{index:02}',environment,repeat) for index in range(1,9)
                 for environment in (('local','connected') if index<=6 else ('connected',)) for repeat in range(3))
