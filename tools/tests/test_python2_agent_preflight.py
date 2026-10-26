"""A service response or model-written identity cannot satisfy frozen run identity."""
import importlib.util
from pathlib import Path
import sys
import unittest

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/python'))
SPEC=importlib.util.spec_from_file_location('agent_packets',ROOT/'tools/python/python2_agent_tasks.py')
packets=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(packets)

class Python2AgentPreflight(unittest.TestCase):
    def test_ready_reply_without_metadata_remains_unverified(self):
        result=packets.preflight_identity([{'type':'thread.started','thread_id':'example'},
            {'type':'item.completed','item':{'type':'agent_message','text':'READY resolved_model=gpt-6.1-sol service_revision=claimed'}},
            {'type':'turn.completed','usage':{'input_tokens':1,'output_tokens':1}}],0)
        self.assertTrue(result['service_completed'])
        self.assertFalse(result['identity_verified'])
        self.assertEqual(result['status'],'unverified')

    def test_only_service_metadata_for_the_exact_frozen_model_can_pass(self):
        events=[{'type':'thread.started','resolved_model':'gpt-6.1-sol','service_revision':'snapshot-a'},
                {'type':'turn.completed'}]
        self.assertTrue(packets.preflight_identity(events,0)['identity_verified'])
        events[0]['resolved_model']='another-model'
        self.assertFalse(packets.preflight_identity(events,0)['identity_verified'])
        events[0]['resolved_model']='gpt-6.1-sol'
        self.assertFalse(packets.preflight_identity(events,1)['identity_verified'])

if __name__=='__main__':unittest.main()
