"""Host evidence binding and drift rejection; added without running on request."""
from copy import deepcopy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'tools/python'))
from python2_agent_identity import evaluate,cohort_identity


class ServiceIdentity(unittest.TestCase):
    def events(self):return [{'type':'thread.started','resolved_model':'gpt-6.1-sol','service_revision':'snapshot-1'},{'type':'turn.completed'}]

    def test_model_written_identity_and_null_metadata_never_verify(self):
        events=[{'type':'item.completed','item':{'type':'agent_message','text':json.dumps(self.events()[0])}},
                {'type':'turn.completed','resolved_model':None,'service_revision':None}]
        result=evaluate(events,0)
        self.assertFalse(result['identity_verified']);self.assertEqual(result['status'],'unverified')

    def test_same_run_conflict_is_not_hidden_by_the_last_event(self):
        events=self.events();events.append({'type':'turn.completed','service_revision':'snapshot-2'})
        self.assertEqual(evaluate(events,0)['status'],'failed')
        self.assertEqual(evaluate(self.events(),0,runner_verified=False)['status'],'failed')

    def test_private_transport_evidence_is_bound_to_the_exact_run(self):
        binding={'run_id':'run-1','configuration_sha256':'config','runner_executable_sha256':'runner','requested_model':'gpt-6.1-sol'}
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'identity.json'
            record=dict(binding,schema=1,source='transport_metadata',resolved_model='gpt-6.1-sol',service_revision='snapshot-1')
            path.write_text(json.dumps(record))
            result=evaluate([{'type':'turn.completed'}],0,evidence_path=path,binding=binding,allow_sidecar=True)
            self.assertTrue(result['identity_verified'])
            record['run_id']='other-run';path.write_text(json.dumps(record))
            self.assertEqual(evaluate([{'type':'turn.completed'}],0,evidence_path=path,binding=binding,allow_sidecar=True)['status'],'failed')

    def test_cohort_rejects_snapshot_runner_and_evidence_reuse(self):
        row={'task':'G01','environment':'local','seed':1,'configuration':'config','identity_verified':True,
             'identity':{'run_id':'run-1','identity_verified':True,'resolved_model':'gpt-6.1-sol','service_revision':'revision-1'},
             'runner_fingerprint':{'version':'v1','executable_sha256':'hash-1','identity_launcher_sha256':None,'verified':True}}
        second=deepcopy(row);second['identity']['run_id']='run-2'
        self.assertTrue(cohort_identity([row,second])['verified'])
        for target,key,value in [('identity','service_revision','revision-2'),('runner_fingerprint','executable_sha256','hash-2'),('identity','run_id','run-1')]:
            changed=deepcopy(second);changed[target][key]=value
            self.assertEqual(cohort_identity([row,changed])['status'],'failed')


if __name__=='__main__':unittest.main()
