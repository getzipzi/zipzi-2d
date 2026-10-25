"""Fixed, reproducible agent task inputs. Hidden expectations stay outside packets."""
from dataclasses import dataclass
from uuid import UUID, uuid5

FIXTURE_VERSION='p3a-agent-fixture-1'
VERIFIER_VERSION='p3a-independent-verifier-1'
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
    hidden={'target_id':uid('target'),'other_ids':[uid('other-0'),uid('other-1')],
            'expected_sample_x':7.5,'expected_pixels':[7,255,9,255],
            'required_controller_events':{'G03':['source_changed_between_pages'],'G04':['plan_source_changed','ordinary_write_failed'],'G07':['create_reply_dropped','apply_reply_dropped'],'G08':['commit_reply_dropped','terminal_reply_dropped']}.get(task_id,[])}
    return Task(task_id,environment,seed,data,hidden)


def matrix():
    return tuple(task(f'G{index:02}',environment,repeat) for index in range(1,9)
                 for environment in (('local','connected') if index<=6 else ('connected',)) for repeat in range(3))
