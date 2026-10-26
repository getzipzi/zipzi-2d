"""Functional thresholds and hard stops decide acceptance; service metadata does not."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'tools/python'))
from python2_agent_matrix import summarize


class AgentMatrix(unittest.TestCase):
    def rows(self):
        return [{'task':f'G{i:02}','environment':environment,'seed':repeat,
                 'functional_passed':True,'identity_verified':False,'configuration':'fixed-config',
                 'runner_fingerprint':{'version':'runner-1','executable_sha256':'runner-hash','identity_launcher_sha256':None,'verified':True},
                 'identity':{'run_id':f'{i}-{environment}-{repeat}','resolved_model':'gpt-6.1-sol','service_revision':'revision-1','identity_verified':False},
                 'input_tokens':1,'output_tokens':2,'total_tokens':3,'verification':{'hard_stop':[]}}
                for i in range(1,9) for environment in (('local','connected') if i<=6 else ('connected',)) for repeat in range(3)]

    def test_38_successes_with_two_per_environment_pass_without_service_identity(self):
        rows=self.rows()
        for index in (0,3,6,9):rows[index]['functional_passed']=False
        result=summarize(rows)
        self.assertTrue(result['functional_gate_passed']);self.assertTrue(result['overall_passed'])
        self.assertEqual(result['model'],{'id':'gpt-6.1-sol','reasoning_effort':'high'})
        for row in rows:
            row.pop('identity');row.pop('runner_fingerprint');row.pop('identity_verified')
        self.assertTrue(summarize(rows)['overall_passed'])

    def test_old_identity_failures_and_service_drift_do_not_change_functional_verdict(self):
        rows=self.rows()
        rows[0]['identity'].update(status='failed',service_revision='another-revision')
        rows[0]['runner_fingerprint']['verified']=False
        self.assertTrue(summarize(rows)['overall_passed'])

    def test_total_score_does_not_hide_an_environment_with_only_one_success(self):
        rows=self.rows()
        for index in (0,1,6,9):rows[index]['functional_passed']=False
        result=summarize(rows)
        self.assertEqual(result['functional_successes'],38)
        self.assertFalse(result['functional_gate_passed'])
        self.assertFalse(result['overall_passed'])

    def test_hard_stop_cannot_be_outvoted_by_successful_runs(self):
        rows=self.rows();rows[0]['verification']['hard_stop']=['unknown_reported_as_success']
        result=summarize(rows)
        self.assertFalse(result['functional_gate_passed'])
        self.assertFalse(result['overall_passed'])
        self.assertEqual(result['hard_stop'],['unknown_reported_as_success'])


if __name__=='__main__':unittest.main()
