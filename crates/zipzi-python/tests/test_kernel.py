"""V4 Local/Connected parity and recovery against a real Runtime process."""
from contextlib import contextmanager
from dataclasses import FrozenInstanceError
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from uuid import uuid4
import zipzi
from zipzi import values as v

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / 'target/debug/examples/local_host_fixture'

@contextmanager
def connected(drop='', **settings):
    with tempfile.TemporaryDirectory(prefix='zipzi-v4-', dir='/tmp') as directory:
        env = dict(os.environ, ZIPZI_RUNTIME_DIR=directory, ZIPZI_RUNTIME_PROFILE='test', ZIPZI_TEST_DROP_RESPONSE=drop, **settings)
        process = subprocess.Popen([str(FIXTURE)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            endpoint = Path(directory) / 'test.sock'
            deadline = time.monotonic()+10
            while not endpoint.exists():
                if process.poll() is not None: raise AssertionError(process.stderr.read().decode())
                if time.monotonic()>deadline: raise TimeoutError('Fixture startup')
                time.sleep(.01)
            with zipzi.connect(endpoint=endpoint, timeout=2) as runtime:
                yield runtime
        finally:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
            process.stderr.close()


def create(id=None, name='p'):
    return v.CommandCreateParameter(spec=v.ParameterCreateSpec(id=id, name=name, minimum=-1, maximum=1, default_value=0))


def query(id):
    return v.QueryObjects(addresses=(v.ObjectAddress(kind=v.ObjectKind.PARAMETER,id=id),))


class Values(unittest.TestCase):
    def test_nested_values_are_owned_immutable_and_roundtrip(self):
        commands=[create()]
        source=v.DraftSource(draft=str(uuid4()),base=v.Version(session=str(uuid4()),generation=0,revision=0),sequence=0)
        operation=v.OperationBatch(expected=source,commands=commands)
        commands.clear()
        self.assertEqual(len(operation.commands),1)
        self.assertEqual(v.Operation.from_wire(operation.to_wire()),operation)
        with self.assertRaises(FrozenInstanceError): operation.commands=()
        with self.assertRaises(ValueError): v.Vec2(x=float('nan'),y=0)
        with self.assertRaises(ValueError): v.Vec2(x=1e40,y=0)
        with self.assertRaises(TypeError): v.Version(session=str(uuid4()),generation=True,revision=0)


class Lifecycle:
    factory = staticmethod(zipzi.local)
    def test_competing_drafts_history_and_exact_query_source(self):
        with self.factory() as runtime:
            session=runtime.sessions.create()
            baseline=session.source
            first=session.draft(); second=session.draft()
            id=str(uuid4()); first.apply(create(id))
            first.publish()
            self.assertEqual(session.query(query(id)).value['records'][0]['value']['name'],'p')
            with self.assertRaises(zipzi.SdkFailure) as caught: second.apply(create())
            self.assertEqual(caught.exception.code,'STALE_SOURCE'); self.assertEqual(second.state,'ended')
            session.undo(); session.redo()
            with self.assertRaises(zipzi.SdkFailure): session.query(v.QuerySummary(),source=baseline)
            self.assertEqual(session.info()['history']['undo_steps'],1)

    def test_preview_refusal_preserves_last_candidate_ordinary_failure_ends_it(self):
        with self.factory() as runtime:
            session=runtime.sessions.create(); draft=session.draft(); id=str(uuid4())
            draft.apply(create(id)); previous=draft.source
            with self.assertRaises(zipzi.SdkFailure): draft.preview(v.CommandRename(target=v.ObjectAddress(kind=v.ObjectKind.PARAMETER,id='missing'),name='bad'))
            self.assertEqual(draft.source,previous); self.assertEqual(draft.state,'ready')
            self.assertEqual(draft.query(query(id)).value['records'][0]['value']['name'],'p')
            draft.preview(v.CommandRename(target=v.ObjectAddress(kind=v.ObjectKind.PARAMETER,id=id),name='good'))
            self.assertEqual(draft.source.sequence,previous.sequence+1)
            with self.assertRaises(zipzi.SdkFailure): draft.apply(create(id))
            self.assertEqual(draft.state,'ended')
            self.assertEqual(session.info()['history']['undo_steps'],0)

    def test_async_mutation_save_open_and_captured_pixels(self):
        with self.factory() as runtime, tempfile.TemporaryDirectory(prefix='zipzi-project-') as directory:
            session=runtime.sessions.create(); draft=session.draft(); id=str(uuid4())
            with draft.apply_async(create(id)) as job:
                job.wait(timeout=5)
            asset=str(uuid4())
            draft.apply(v.CommandCreateAsset(id=asset,name='pixel'),v.CommandReplaceImage(id=asset,input=v.ImageInputRgba(width=1,height=1,bytes=(255,0,0,255))))
            draft.publish()
            path=Path(directory)/'native.zipzi'
            with session.save(path) as job: self.assertTrue(job.wait(timeout=10)['published'])
            self.assertFalse(session.info()['dirty'])
            session.undo(); self.assertTrue(session.info()['dirty'])
            session.redo(); self.assertFalse(session.info()['dirty'])
            with session.open(path) as job: self.assertEqual(job.wait(timeout=10)['kind'],'opened')
            self.assertEqual(session.info()['history']['undo_steps'],0)
            self.assertEqual(session.query(query(id)).value['records'][0]['value']['name'],'p')


class LocalLifecycle(Lifecycle,unittest.TestCase): pass
class ConnectedLifecycle(Lifecycle,unittest.TestCase): factory=staticmethod(connected)


class Recovery(unittest.TestCase):
    def test_lost_publish_response_recovers_once_and_finishes_draft(self):
        with connected(drop='publish') as runtime:
            session=runtime.sessions.create(); draft=session.draft(); draft.apply(create())
            with self.assertRaises(zipzi.OutcomeUnknown) as caught: draft.publish()
            result=zipzi.recover_call(caught.exception)
            self.assertEqual(result['kind'],'commit'); self.assertEqual(draft.state,'ended')
            self.assertEqual(session.info()['history']['undo_steps'],1)

    def test_lost_create_response_preserves_original_projection(self):
        with connected(drop='create_session') as runtime:
            with self.assertRaises(zipzi.OutcomeUnknown) as caught: runtime.sessions.create()
            session=zipzi.recover_call(caught.exception)
            self.assertIsInstance(session,zipzi.Session)
            self.assertEqual(len(runtime.sessions.list()),1)

    def test_lost_close_response_can_be_recovered(self):
        with connected(drop='release_client') as runtime:
            with self.assertRaises(zipzi.OutcomeUnknown) as caught: runtime.close()
            self.assertIsNone(zipzi.recover_call(caught.exception))
            runtime.close()


if __name__=='__main__': unittest.main()
