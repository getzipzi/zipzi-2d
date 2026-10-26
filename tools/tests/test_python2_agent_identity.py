"""Optional service metadata records observations without an identity verdict."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'tools/python'))
from python2_agent_identity import evaluate


class ServiceMetadata(unittest.TestCase):
    def test_missing_metadata_does_not_make_a_completed_service_unavailable(self):
        result=evaluate([{'type':'turn.completed'}],0)
        self.assertTrue(result['service_completed'])
        self.assertEqual(result['mode'],'report_only')
        self.assertEqual(result['resolved_models'],[])
        self.assertEqual(result['service_revisions'],[])

    def test_lifecycle_observations_are_reported_without_mismatch_failures(self):
        result=evaluate([
            {'type':'thread.started','resolved_model':'model-a','service_revision':'revision-1'},
            {'type':'turn.completed','resolved_model':'model-b','service_revision':'revision-2'}],0)
        self.assertTrue(result['service_completed'])
        self.assertEqual(result['resolved_models'],['model-a','model-b'])
        self.assertEqual(result['service_revisions'],['revision-1','revision-2'])
        self.assertNotIn('failures',result)

    def test_model_messages_and_malformed_values_are_not_service_metadata(self):
        result=evaluate([
            {'type':'item.completed','item':{'type':'agent_message','text':'service_revision=claimed'}},
            {'type':'turn.completed','resolved_model':None,'service_revision':42}],0)
        self.assertEqual(result['resolved_models'],[])
        self.assertEqual(result['service_revisions'],[])


if __name__=='__main__':unittest.main()
