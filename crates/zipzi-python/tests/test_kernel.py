"""V4 Local/Connected parity and recovery against a real Runtime process."""
from contextlib import contextmanager
from dataclasses import FrozenInstanceError
import os
from pathlib import Path
import subprocess
import tempfile
import sys
import time
import unittest
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[3]
if source_client := os.environ.get('ZIPZI_KERNEL_SOURCE_CLIENT'):
    sys.path.insert(0, source_client)
if native := os.environ.get('ZIPZI_KERNEL_NATIVE'):
    sys.path.insert(0, native)

import zipzi
from zipzi import values as v

FIXTURE = ROOT / 'target/debug/examples' / ('local_host_fixture.exe' if os.name == 'nt' else 'local_host_fixture')


@contextmanager
def connected(drop='', **settings):
    with tempfile.TemporaryDirectory(prefix='zipzi-v4-', dir=None if os.name == 'nt' else '/tmp') as directory:
        profile = 'test'
        env = dict(os.environ, ZIPZI_RUNTIME_DIR=directory, ZIPZI_RUNTIME_PROFILE=profile,
                   ZIPZI_TEST_DROP_RESPONSE=drop, **settings)
        for key in ('ZIPZI_DEV_SESSION_DIR', 'ZIPZI_DEV_PID_FILE', 'ZIPZI_DEV_READY_FILE', 'ZIPZI_RUNTIME_OWNER_FD'):
            env.pop(key, None)
        if editor := os.environ.get('ZIPZI_KERNEL_EDITOR_HOST'):
            command = [editor, '--no-window']
        else:
            command = [os.environ.get('ZIPZI_KERNEL_HOST', str(FIXTURE))]
        if os.name == 'nt':
            from zipzi._windows_pipe import endpoint_name
            endpoint = endpoint_name(Path(directory), profile)
        else:
            endpoint = Path(directory) / f'{profile}.sock'
        process = subprocess.Popen(command, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 30
            while True:
                if process.poll() is not None:
                    raise AssertionError(process.stderr.read().decode())
                try:
                    runtime = zipzi.connect(endpoint=endpoint, timeout=2)
                    break
                except (FileNotFoundError, ConnectionRefusedError):
                    if time.monotonic() >= deadline:
                        raise TimeoutError('Host startup')
                    time.sleep(.01)
            with runtime:
                yield runtime
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
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
    def test_bulk_image_roundtrip_and_frozen_sampling(self):
        import json
        with self.factory() as runtime:
            session = runtime.sessions.create()
            id = str(uuid4())
            with runtime.blob(4) as upload:
                self.assertEqual(upload.append(0, bytes([11,22,33,255])), 4)
                self.assertEqual(upload.seal(), 4)
                with session.draft() as draft:
                    draft.apply(v.CommandCreateAsset(id=id, name='pixel'))
                    draft.replace_image_blob(id, upload, width=1, height=1)
            with session.image_blob(id) as image:
                self.assertEqual(image.info['width'], 1)
                self.assertEqual(image.read(0,4), bytes([11,22,33,255]))
            source = session.source
            request = v.SampleRequest(input=v.SampleInputParameters(values=({}, {})), quality=True)
            with session.samples(request, source=source) as job:
                result = job.wait(timeout=10)
                self.assertEqual(result['source'], source.to_wire())
                with job.blob() as blob:
                    value = json.loads(blob.read(0, blob.length))
                self.assertEqual(len(value['frames']), 2)
                self.assertEqual(value['source'], source.to_wire())
            animation = v.SampleRequest(input=v.SampleInputAnimation(times=(0.,.1)))
            with session.samples(animation) as job:
                job.wait(timeout=10)
                with job.blob() as blob:
                    value = json.loads(blob.read(0, blob.length))
                self.assertEqual(len(value['frames']), 2)
                self.assertIsNotNone(value['frames'][1]['animation'])

    def test_mesh_records_candidate_samples_and_history(self):
        import json

        with self.factory() as runtime:
            session = runtime.sessions.create()
            mesh, asset = str(uuid4()), str(uuid4())
            vertices = (42, 7, 99)
            positions = (v.Vec2(x=0, y=0), v.Vec2(x=1, y=0), v.Vec2(x=0, y=1))
            geometry = v.MeshGeometry(vertex_ids=vertices, positions=positions, uvs=positions,
                                      triangles=(vertices,))
            selection = v.QueryObjects(addresses=(v.ObjectAddress(kind=v.ObjectKind.MESH, id=mesh),))
            with session.draft('Create textured mesh') as draft:
                draft.apply(v.CommandCreateAsset(id=asset, name='pixel'),
                    v.CommandReplaceImage(id=asset, input=v.ImageInputRgba(width=1,height=1,bytes=(255,0,0,255))),
                    v.CommandCreateMesh(spec=v.MeshCreateSpec(id=mesh, name='Triangle',
                        geometry=geometry, texture_asset_id=asset)))
            original = session.query(selection)
            self.assertEqual(v.Mesh.from_wire(original.value['records'][0]['value']).vertex_ids, vertices)
            with session.image_blob(asset) as previous, session.draft('Vertex and pixels') as draft:
                draft.apply(v.CommandVertexPositions(mesh=mesh, updates=((7, v.Vec2(x=2,y=0)),)),
                    v.CommandReplaceImage(id=asset, input=v.ImageInputRgba(width=1,height=1,bytes=(0,0,255,255))))
                self.assertEqual(session.query(selection), original)
                candidate = v.QuerySourceDraft(value=draft.source)
                request = v.SampleRequest(input=v.SampleInputParameters(values=({},)), quality=True)
                with session.samples(request, source=candidate) as job:
                    job.wait(timeout=10)
                    with job.blob() as blob:
                        report = json.loads(blob.read(0, blob.length))
                frame = report['frames'][0]
                self.assertEqual(report['source'], candidate.to_wire())
                self.assertEqual(frame['geometry']['drawables'][0]['positions'][1]['x'], 2)
                self.assertIsNotNone(frame['quality'])
                self.assertEqual(previous.read(0,4), bytes((255,0,0,255)))
            self.assertEqual(session.info()['history']['undo_steps'], 2)
            session.undo()
            self.assertEqual(session.query(selection).content, original.content)
            with session.image_blob(asset) as restored:
                self.assertEqual(restored.read(0,4), bytes((255,0,0,255)))
            session.redo()
            with session.image_blob(asset) as restored:
                self.assertEqual(restored.read(0,4), bytes((0,0,255,255)))

    def test_event_cursors_are_independent_and_reads_do_not_publish(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            first = v.Version.from_wire(session.subscribe()['cursor'])
            id = str(uuid4())
            with session.draft() as draft:
                draft.apply(create(id))
            second = v.Version.from_wire(session.subscribe()['cursor'])
            session.query(query(id))
            self.assertEqual(session.events(second), ())
            session.undo()
            session.redo()
            first_read = session.events(first)
            self.assertEqual([event['kind'] for event in first_read], ['commit','undo','redo'])
            self.assertEqual([event['kind'] for event in session.events(second)], ['undo','redo'])
            self.assertEqual(session.events(first), first_read)

    def test_observation_uses_explicit_view_and_bulk_png(self):
        import json
        import struct
        with self.factory() as runtime:
            if 'gpu_observation' not in runtime.capabilities(): self.skipTest('CPU-only host')
            session = runtime.sessions.create()
            source = session.source
            view = v.ObservationView(width=32, height=32, roi=(-1.,-1.,1.,1.))
            with session.observe(v.SampleInputParameters(values=({},)), view) as job:
                job.wait(timeout=30)
                with job.blob() as blob:
                    payload = blob.read(0,blob.length)
            length, = struct.unpack('>I',payload[:4])
            info = json.loads(payload[4:4+length])
            self.assertEqual(info['source'], source.to_wire())
            self.assertEqual(payload[4+length:4+length+8], b'\x89PNG\r\n\x1a\n')

    def test_import_psd_replaces_content_and_captures_resources(self):
        import shutil
        with self.factory() as runtime, tempfile.TemporaryDirectory(prefix='zipzi-import-') as directory:
            source=Path(directory)/'input.psd'
            shutil.copyfile(ROOT/'crates/zipzi-psd/tests/fixtures/layered.psd',source)
            session=runtime.sessions.create()
            with session.draft() as draft: draft.apply(create())
            before=session.version
            with session.import_project(source,v.ImportFormat.PSD) as job:
                result=job.wait(timeout=10)
            self.assertEqual(result['kind'],'imported')
            self.assertEqual(session.version.generation,before.generation+1)
            self.assertEqual(session.info()['history']['undo_steps'],0)
            self.assertTrue(session.info()['dirty'])
            with self.assertRaises(zipzi.SdkFailure) as failure:
                session.save(source)
            self.assertEqual(failure.exception.code,'INVALID_DESTINATION')
            with session.export(Path(directory)/'runtime', v.ExportFormat.RUNTIME) as job:
                exported = job.wait(timeout=30)
            self.assertTrue(exported['receipt']['published'])
            self.assertTrue((Path(directory)/'runtime').is_dir())
            source.unlink()
            with session.save(Path(directory)/'imported.zipzi') as job:
                self.assertTrue(job.wait(timeout=10)['published'])

    def test_collection_capabilities_share_exact_published_and_draft_sources(self):
        with self.factory() as runtime:
            session=runtime.sessions.create()
            part=str(uuid4())
            with session.draft() as draft:
                draft.apply(v.CommandCreatePart(value=v.Part(id=part,runtime_id=part,name='owner',parent_id='',enabled=True,draw_order=0)))
            baseline=session.source
            query=v.QueryCollectionCapabilities(target=v.CollectionTargetOrganization(value=v.OrganizationCollectionPartMembers(part_id=part)))
            before=session.query(query)
            draft=session.draft()
            draft.apply(v.CommandEditorState(value=v.EditorState(objects={part:v.ObjectEditorState(locked=True)})))
            after=draft.query(query)
            self.assertNotEqual(before.value['create'],after.value['create'])
            self.assertEqual(session.query(query).value,before.value)
            draft.publish()
            self.assertEqual(session.query(query).value,after.value)
            with self.assertRaises(zipzi.SdkFailure) as failure:
                session.query(query,source=baseline)
            self.assertEqual(failure.exception.code,'STALE_SOURCE')

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

    def test_pagination_rejects_an_old_source_after_publication(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            ids = [str(uuid4()) for _ in range(3)]
            with session.draft() as draft:
                draft.apply(*(create(id) for id in ids))
            page = session.query(v.QueryCollection(kind=v.ObjectKind.PARAMETER, offset=0, limit=2))
            self.assertEqual([item['address']['id'] for item in page.value['records']], ids[:2])
            continuation = v.QueryCollection(kind=v.ObjectKind.PARAMETER,
                                             offset=page.value['next_offset'], limit=2)
            last = session.query(continuation, source=page.source)
            self.assertEqual([item['address']['id'] for item in last.value['records']], ids[2:])
            self.assertIsNone(last.value['next_offset'])
            session.undo()
            with self.assertRaises(zipzi.SdkFailure) as failure:
                session.query(continuation, source=page.source)
            self.assertEqual(failure.exception.code, 'STALE_SOURCE')

    def test_detach_preserves_dirty_session_until_explicit_close(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            with session.draft() as draft:
                draft.apply(create())
            source = session.source
            session.detach()
            with self.assertRaises(zipzi.SdkFailure):
                session.query(v.QuerySummary(), source=source)
            reattached = runtime.sessions.attach(session.id)
            self.assertEqual(reattached.source, source)
            self.assertTrue(reattached.info()['dirty'])
            with self.assertRaises(zipzi.SdkFailure) as failure:
                reattached.close()
            self.assertEqual(failure.exception.code, 'UNSAVED_CHANGES')
            reattached.close(discard_modified=True)
            self.assertFalse(runtime.sessions.list())

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
    def test_batch_recovery_updates_one_candidate_sequence_and_publishes_once(self):
        with connected('batch') as runtime:
            session = runtime.sessions.create()
            id = str(uuid4())
            cursor = session.version
            draft = session.draft()
            with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                draft.apply(create(id))
            self.assertEqual(draft.source.sequence, 0)
            zipzi.recover_call(lost.exception)
            self.assertEqual(draft.source.sequence, 1)
            self.assertEqual(draft.query(query(id)).value['records'][0]['value']['name'], 'p')
            draft.publish()
            self.assertEqual(len(session.events(cursor)), 1)
            self.assertEqual(session.info()['history']['undo_steps'], 1)

    def test_event_gap_requires_resubscription(self):
        with connected(ZIPZI_TEST_EVENT_COUNT='1') as runtime:
            session = runtime.sessions.create()
            cursor = session.version
            for _ in range(3):
                with session.draft() as draft:
                    draft.apply(create())
            with self.assertRaises(zipzi.SdkFailure) as error:
                session.events(cursor)
            self.assertEqual(error.exception.code, 'EVENT_GAP')
            self.assertEqual(session.events(v.Version.from_wire(session.subscribe()['cursor'])), ())

    def test_binary_upload_and_read_recovery_keep_original_projection(self):
        for operation in ('append_blob_binary', 'read_blob_binary'):
            with self.subTest(operation=operation), connected(operation) as runtime:
                with runtime.blob(4) as blob:
                    try:
                        end = blob.append(0,b'abcd')
                    except zipzi.OutcomeUnknown as unknown:
                        end = zipzi.recover_call(unknown)
                    self.assertEqual(end,4)
                    blob.seal()
                    try:
                        value = blob.read(1,2)
                    except zipzi.OutcomeUnknown as unknown:
                        value = zipzi.recover_call(unknown)
                    self.assertEqual(value,b'bc')

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
