"""External fixture decisions and independent evidence rejection, without model calls."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/python'))
from python2_agent_observer import ObservationChannel,controller_events


class ObservationProtocol(unittest.TestCase):
    def test_local_mutation_is_not_credited_until_real_transition_is_observed(self):
        with tempfile.TemporaryDirectory() as directory:
            private=Path(directory)
            (private/'manifest.json').write_text(json.dumps({'task':'G03','environment':'local'}))
            (private/'verifier-task.json').write_text(json.dumps({'document_id':'document'}))
            channel=ObservationChannel(private);stream=channel.child.makefile('rw')
            def exchange(event):
                stream.write(json.dumps(dict(event,pid=1))+'\n');stream.flush()
                return json.loads(stream.readline())
            self.assertEqual(exchange({'kind':'public_return','domain':'meshes','path':'Meshes.list','value':{'next_cursor':None,'source':{'edit':None}}}),{})
            event={'kind':'public_return','domain':'meshes','path':'Meshes.list','session':{'runtime':'r','serial':1},'value':{'next_cursor':'cursor','source':{'edit':None}}}
            self.assertEqual(exchange(event)['command'],'publish_fixture_parameter')
            self.assertEqual(controller_events(private),set())
            before={'version':1,'history':[0,0]};after={'version':2,'history':[1,0]}
            exchange({'kind':'fixture_publication','event':'source_changed_between_pages','before':before,'after':after})
            self.assertEqual(controller_events(private),{'source_changed_between_pages'})
            self.assertEqual(exchange(event),{})
            stream.close();channel.close()


class IndependentCoverage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec=importlib.util.spec_from_file_location('external_verifier',ROOT/'fixtures/python_sdk_agent/verify.py')
        cls.verifier=importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules,{'zipzi':SimpleNamespace()}):spec.loader.exec_module(cls.verifier)

    def evidence(self,private,*,intact=True,wrong_other=False):
        def state(history,meshes):return {'document_id':'document','info':{'id':{'runtime':'r','serial':1},'history':[history,0]},'meshes':meshes}
        events=[{'kind':'observer_started','pid':1},
            {'kind':'state','pid':1,'phase':'session_acquired','state':state(0,{})},
            {'kind':'state','pid':1,'phase':'before_close','state':state(1,{'target':{'name':'Done'},'other':{'name':'Wrong' if wrong_other else 'Face'}})},
            {'kind':'public_return','pid':1,'return_type':'SaveReceipt','value':{'durable':True}},
            {'kind':'public_return','pid':1,'return_type':'Receipt','value':{}},
            {'kind':'public_exception','pid':1,'error_type':'OutcomeUnknown'},
            {'kind':'observer_finished','pid':1,'intact':intact,'exit_status':0}]
        (private/'observer-events.ndjson').write_text(''.join(json.dumps(e)+'\n' for e in events))
        (private/'manifest.json').write_text(json.dumps({'hidden':{'target_id':'target'}}))

    def test_trace_tampering_cannot_be_replaced_by_correct_saved_content(self):
        with tempfile.TemporaryDirectory() as directory:
            private=Path(directory);self.evidence(private,intact=False)
            checks,_,_=self.verifier.observed_checks({'document_id':'document'},'G08',private)
            self.assertFalse(checks['observed_workflow_completed'])

    def test_wrong_object_publication_is_a_hard_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            private=Path(directory);self.evidence(private,wrong_other=True)
            _,hard,_=self.verifier.observed_checks({'document_id':'document'},'G01',private)
            self.assertEqual(hard,['wrong_object_modified'])

    def test_unresolved_original_call_cannot_be_confirmed_by_saved_files(self):
        with tempfile.TemporaryDirectory() as directory:
            private=Path(directory);self.evidence(private)
            path=private/'observer-events.ndjson';events=[json.loads(line) for line in path.read_text().splitlines()]
            events[-1]['pending_recovery']=[{'type':'Call','pending':{'request_id':7}}]
            path.write_text(''.join(json.dumps(e)+'\n' for e in events))
            checks,_,_=self.verifier.observed_checks({'document_id':'document'},'G08',private)
            self.assertFalse(checks['no_unknown_recovery_left'])


if __name__=='__main__':unittest.main()
