"""Supplement a frozen cohort's lifecycle grading using native Runtime identity.

G06 lifecycle and the G04 ordinary-write dispatch predicate are revised. CLI, prompts, tool execution,
raw events, original scores and all other checks remain untouched. No model or
SDK task is rerun. A Connected host can outlive/rebind multiple Python clients.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from python2_agent_matrix import summarize


import importlib.util
_spec=importlib.util.spec_from_file_location('zipzi_agent_criteria',Path(__file__).resolve().parents[2]/'fixtures/python_sdk_agent/criteria.py')
_criteria=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(_criteria)
native_lifecycle=_criteria.native_lifecycle
ordinary_failure=_criteria.ordinary_failure


def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()




def review(root:Path,output:Path):
    output.mkdir(parents=True,exist_ok=False);rows=[];changes=[]
    for path in sorted(root.glob('*.controller/result.json')):
        result=json.loads(path.read_text())
        if result['task'] in {'G04','G06'}:
            source=path.parent/'observer-events.ndjson';events=[json.loads(line) for line in source.read_text().splitlines()]
            if result['task']=='G06':checks,details=native_lifecycle(events)
            else:
                checks={'ordinary_failure_aborted':ordinary_failure(events)}
                details={'rule':'STALE_FIELD on an aborted Edit through either a domain facade or unified Edit.apply'}
            original_checks=result['verification']['checks'];revised_checks=dict(original_checks,**checks)
            revised=all(revised_checks.values()) and result['verification']['coverage_complete'] and not result['verification'].get('hard_stop') and not result['failures']
            audit={'revision':'public-runtime-and-dispatch-1','original_result_sha256':sha(path),
                'observation_sha256':sha(source),'reviewer_sha256':sha(Path(__file__)),
                'criteria_sha256':sha(Path(_criteria.__file__)),
                'original_functional_passed':result['functional_passed'],'reviewed_functional_passed':revised,
                'original_lifecycle_checks':{key:original_checks[key] for key in checks},'reviewed_lifecycle_checks':checks,'details':details}
            (output/(path.parent.name+'.json')).write_text(json.dumps(audit,indent=2)+'\n')
            if revised!=result['functional_passed']:changes.append(path.parent.name)
            result=dict(result,functional_passed=revised)
        rows.append(result)
    summary=summarize(rows);summary['grading_revision']='public-runtime-and-dispatch-1';summary['changed_results']=changes
    summary['original_summary_sha256']=sha(root/'summary.json');summary['execution_freeze_sha256']=sha(root/'freeze.json')
    (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    return summary


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    args=p.parse_args();print(json.dumps(review(args.root,args.output)))
