"""Real shell isolation and external reply-loss selection without model calls/builds."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/python'))
from python2_agent_shell import policy,execute
from python2_agent_controller import Controller
from python2_agent_tasks import RunRecorder
from python2_acceptance import token_usage


class AgentController(unittest.TestCase):
    def test_missing_service_revision_does_not_fail_a_functional_run_record(self):
        identity={'task':'G02','environment':'local','seed':314360,'model':'gpt-6.1-sol',
            'runner_version':'test-runner','package_hashes':{'wheel':'a'},
            'document_hashes':{'guide':'b'},'prompt_hash':'c'}
        with tempfile.TemporaryDirectory() as directory:
            recorder=RunRecorder(Path(directory)/'run',identity)
            report=recorder.finish({'verified':True,'evidence_digest':'independent',
                'coverage_complete':True},set(),set()).read_text()
            self.assertIn('Status: passed',report)
            self.assertIn('Failures: none',report)

    def test_large_token_usage_is_reported_without_failing_the_run(self):
        identity={'task':'G02','environment':'local','seed':314360,'model':'gpt-6.1-sol',
            'service_revision':'test-revision','runner_version':'test-runner','package_hashes':{'wheel':'a'},
            'document_hashes':{'guide':'b'},'prompt_hash':'c'}
        with tempfile.TemporaryDirectory() as directory:
            recorder=RunRecorder(Path(directory)/'run',identity)
            recorder.event('usage',{'input_tokens':1000000,'output_tokens':100000})
            recorder.event('usage',{'input_tokens':2000000,'output_tokens':200000})
            report=recorder.finish({'verified':True,'evidence_digest':'independent',
                'coverage_complete':True},set(),set()).read_text()
            self.assertIn('Status: passed',report)
            self.assertIn('3000000 input / 300000 output / 3300000 total',report)
            self.assertIn('Failures: none',report)

    def test_missing_and_partial_usage_are_reported_as_unavailable(self):
        self.assertEqual(token_usage([]),dict(input_tokens=None,output_tokens=None,total_tokens=None))
        self.assertEqual(token_usage([{'input_tokens':42}]),dict(input_tokens=42,output_tokens=None,total_tokens=None))
        self.assertEqual(token_usage([{'input_tokens':0,'output_tokens':0}]),dict(input_tokens=0,output_tokens=0,total_tokens=0))
        self.assertEqual(token_usage([{'input_tokens':-1,'output_tokens':2}]),dict(input_tokens=None,output_tokens=2,total_tokens=None))
        identity={'task':'G02','environment':'local','seed':314360,'model':'gpt-6.1-sol',
            'service_revision':'test-revision','runner_version':'test-runner','package_hashes':{'wheel':'a'},
            'document_hashes':{'guide':'b'},'prompt_hash':'c'}
        with tempfile.TemporaryDirectory() as directory:
            recorder=RunRecorder(Path(directory)/'run',identity)
            report=recorder.finish({'verified':True,'evidence_digest':'independent',
                'coverage_complete':True},set(),set()).read_text()
            self.assertIn('Status: passed',report)
            self.assertIn('unavailable input / unavailable output / unavailable total',report)

    def test_correct_files_without_complete_coverage_cannot_pass_a_formal_record(self):
        identity={'task':'G08','environment':'connected','seed':314961,'model':'gpt-6.1-sol',
            'service_revision':'test-revision','runner_version':'test-runner','package_hashes':{'wheel':'a'},
            'document_hashes':{'guide':'b'},'prompt_hash':'c'}
        with tempfile.TemporaryDirectory() as directory:
            for complete,expected in ((False,'failed'),(True,'passed')):
                recorder=RunRecorder(Path(directory)/str(complete),identity)
                report=recorder.finish({'verified':True,'evidence_digest':'independent-files',
                    'coverage_complete':complete},set(),set())
                self.assertIn('Status: '+expected,report.read_text())

    def controller(self,task):
        value=object.__new__(Controller);value.task=task;value.events=set();value.seen_requests=set()
        import threading
        value.lock=threading.Lock();value.record=Mock()
        def mutate(event):value.events.add(event)
        value.mutate=Mock(side_effect=mutate)
        return value

    def test_only_actual_terminal_job_reply_is_dropped_once(self):
        c=self.controller('G08')
        request={'operation':{'op':'job'}}
        self.assertFalse(c.intercept(request,{'ok':{'result':None}}))
        self.assertFalse(c.intercept(request,{'error':{'code':'BUSY'}}))
        self.assertTrue(c.intercept(request,{'ok':{'result':{'ok':{'changed':True}}}}))
        self.assertFalse(c.intercept(request,{'ok':{'result':{'ok':{'changed':True}}}}))
        self.assertEqual(c.events,{'terminal_reply_dropped'})
        self.assertTrue(c.intercept({'operation':{'op':'commit'}},{'ok':{'job':7}}))
        self.assertFalse(c.intercept({'operation':{'op':'commit'}},{'ok':{'job':7}}))

    def test_failed_writes_do_not_consume_recovery_faults(self):
        c=self.controller('G07');request={'operation':{'op':'apply_authoring'}}
        self.assertFalse(c.intercept(request,{'error':{'code':'INVALID'}}))
        self.assertTrue(c.intercept(request,{'ok':{'ids':['id']}}))
        self.assertTrue(c.intercept(request,{'ok':{'ids':[]}}))
        self.assertFalse(c.intercept(request,{'ok':{'ids':[]}}))
        self.assertEqual(c.events,{'create_reply_dropped','apply_reply_dropped'})

    def test_page_publication_requires_an_actual_continuation(self):
        c=self.controller('G03');request={'operation':{'op':'collection_page'}}
        self.assertFalse(c.intercept(request,{'ok':{'next_cursor':None}}));c.mutate.assert_not_called()
        self.assertFalse(c.intercept(request,{'ok':{'next_cursor':'cursor'}}));c.mutate.assert_called_once()
        c.intercept(request,{'ok':{'next_cursor':'next'}});c.mutate.assert_called_once()

    def test_same_request_recovery_does_not_consume_the_next_write_fault(self):
        c=self.controller('G07')
        create={'client':'client-a','request_id':7,'operation':{'op':'apply_authoring'}}
        self.assertTrue(c.intercept(create,{'ok':{'ids':['mesh']}}))
        self.assertFalse(c.intercept(create,{'ok':{'ids':['mesh']}}))
        self.assertEqual(c.events,{'create_reply_dropped'})
        rename={'client':'client-a','request_id':8,'operation':{'op':'apply_authoring'}}
        self.assertTrue(c.intercept(rename,{'ok':{'ids':[]}}))
        self.assertEqual(c.events,{'create_reply_dropped','apply_reply_dropped'})

    def test_structured_client_identity_is_supported_for_cached_recovery(self):
        c=self.controller('G08')
        request={'client':{'runtime':'runtime-id','serial':3},'request_id':11,'operation':{'op':'commit'}}
        self.assertTrue(c.intercept(request,{'ok':{'job':7}}))
        self.assertFalse(c.intercept(request,{'ok':{'job':7}}))
        self.assertEqual(c.events,{'commit_reply_dropped'})


@unittest.skipUnless(sys.platform=='darwin','macOS Seatbelt acceptance runner')
class AgentIsolation(unittest.TestCase):
    def test_shell_reads_and_heredocs_work_but_secrets_install_writes_and_internet_are_denied(self):
        with tempfile.TemporaryDirectory(prefix='zz-isolation-',dir='/tmp') as directory:
            root=Path(directory).resolve();packet=root/'packet';private=root/'private'
            for p in (packet/'work/tmp',packet/'output',packet/'host',packet/'environment',private):p.mkdir(parents=True,exist_ok=True)
            (private/'secret').write_text('PRIVATE')
            (packet/'task.json').write_text('PUBLIC')
            profile=private/'shell.sb';profile.write_text(policy(packet,Path(sys.base_prefix).resolve(),packet/'work/tmp'))
            def run(command):return execute(command,packet,profile,packet/'work/tmp')
            self.assertEqual(run('cat task.json')['output'],'PUBLIC')
            self.assertEqual(run('cat << EOF\nheredoc\nEOF')['output'],'heredoc\n')
            self.assertEqual(run('echo yes > work/result')['returncode'],0)
            self.assertNotEqual(run('echo no > environment/tamper')['returncode'],0)
            self.assertNotEqual(run('cat '+str(private/'secret'))['returncode'],0)
            self.assertNotEqual(run('cat '+str(ROOT/'README.md'))['returncode'],0)
            python=Path(sys.executable).resolve()
            result=run(str(python)+''' -c 'import socket; socket.create_connection(("1.1.1.1",443),timeout=1)' ''')
            self.assertNotEqual(result['returncode'],0)
            self.assertIn('Operation not permitted',result['output'])


if __name__=='__main__':unittest.main()
