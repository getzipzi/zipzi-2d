"""V4 Local/Connected parity and recovery against a real Runtime process."""
from contextlib import contextmanager
from dataclasses import FrozenInstanceError, replace
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


def documented_example(name, arguments):
    from contextlib import redirect_stdout
    from importlib.resources import files
    import io
    import re
    from unittest.mock import patch
    source = files('zipzi').joinpath('docs/getting-started.md').read_text(encoding='utf-8')
    match = re.search(r'<!-- example: ' + re.escape(name) + r' -->\n```python\n(.*?)\n```', source, re.DOTALL)
    if match is None:
        raise AssertionError('Missing installed example: ' + name)
    output = io.StringIO()
    with patch.object(sys, 'argv', [name + '.py', *arguments]), redirect_stdout(output):
        exec(compile(match[1], name + '.py', 'exec'), {'__name__': '__main__'})
    return output.getvalue()


class Values(unittest.TestCase):
    def test_installed_reference_is_complete_and_readable_offline(self):
        from importlib.resources import files
        from types import ModuleType
        import re

        package = files('zipzi')
        reference = package.joinpath('docs/reference')
        index = reference.joinpath('index.md').read_text(encoding='utf-8')
        for exported in zipzi.__all__:
            self.assertIn('zipzi.' + exported, index)
        for domain in v.__all__:
            module = getattr(v, domain)
            if not isinstance(module, ModuleType):
                continue
            page = reference.joinpath('values-' + domain + '.md').read_text(encoding='utf-8')
            for exported in module.__all__:
                self.assertIn('## zipzi.values.' + exported + '\n', page)
        for page in reference.iterdir():
            text = page.read_text(encoding='utf-8')
            prose = re.sub(r'```.*?```', '', text, flags=re.DOTALL)
            for link in re.findall(r'\]\(([^)]+)\)', prose):
                destination, _, anchor = link.partition('#')
                content = reference.joinpath(destination).read_text(encoding='utf-8')
                if anchor:
                    self.assertIn(f'<a id="{anchor}">', content)

    def test_domain_imports_share_types_and_resolve_runtime_annotations(self):
        from importlib import import_module
        from types import ModuleType
        from typing import get_type_hints
        import pickle
        from zipzi.values.meshes import Mesh, MeshGeometry
        from zipzi.values.parameters import Parameter

        self.assertIs(v.Mesh, Mesh)
        self.assertIs(v.Parameter, Parameter)
        self.assertIs(v.MeshGeometry, MeshGeometry)
        self.assertIs(get_type_hints(v.CommandCreateMesh)['spec'], v.MeshCreateSpec)
        self.assertEqual(get_type_hints(v.MeshGeometry)['positions'], tuple[v.Vec2, ...])
        for name in v.__all__:
            member = getattr(v, name)
            if isinstance(member, ModuleType):
                self.assertIs(member, import_module('zipzi.values.' + name))
                continue
            self.assertTrue(member.__module__.startswith('zipzi.values.'))
            with self.subTest(type=name):
                get_type_hints(member)
        self.assertIn('zipzi.values.meshes.Mesh', zipzi.describe('values.meshes').children)
        self.assertIn('vertex_ids', zipzi.describe('values.meshes.Mesh').format())
        point = v.Vec2(x=1, y=2)
        self.assertEqual(pickle.loads(pickle.dumps(point)), point)

    def test_offline_help_follows_public_paths_and_pages_without_a_host(self):
        from importlib.resources import files
        from unittest.mock import patch
        with patch.object(zipzi, 'local', side_effect=AssertionError('No host for help')), \
             patch.object(zipzi, 'connect', side_effect=AssertionError('No host for help')):
            self.assertIn('zipzi.Session', zipzi.describe().children)
            self.assertIn('zipzi.Runtime.sessions', zipzi.describe('Runtime').children)
            self.assertIn('zipzi.Runtime.sessions.attach', zipzi.describe('Runtime.sessions').children)
            self.assertIn('zipzi.Blob.length', zipzi.describe('Blob').children)
            self.assertIn('Type: int', zipzi.describe('Blob.length').documentation)
            self.assertIn('Type: str', zipzi.describe('Session.id').documentation)
            self.assertIn('Type: WireMap', zipzi.describe('Blob.info').documentation)
            self.assertIn('document', zipzi.describe('Runtime.sessions.create').documentation)
            method = zipzi.describe('Session.query')
            self.assertIn('source', method.signature)
            self.assertIn('records', method.documentation)
            command = zipzi.describe('values.CommandCreateParameter')
            self.assertIn('spec', command.signature)
            self.assertIn('zipzi.values.CommandCreateParameter',
                          zipzi.describe('values.Command', limit=64).children)
            self.assertIn('from_wire', zipzi.describe('values.Parameter.from_wire').path)
            self.assertIn('durable', zipzi.describe('results.SaveReceipt').format())
            self.assertEqual(zipzi.describe('results.SaveReceipt').children, ())
            self.assertIn('zipzi.values.ObjectKind.MESH', zipzi.describe('values.ObjectKind', limit=64).children)
            self.assertIn("'mesh'", zipzi.describe('values.ObjectKind.MESH').format())
            paths, offset = [], 0
            while True:
                page = zipzi.describe('values', offset=offset, limit=7)
                self.assertLessEqual(len(page.children), 7)
                paths.extend(page.children)
                if page.next_offset is None: break
                offset = page.next_offset
            self.assertEqual(len(paths), len(set(paths)))
            self.assertEqual(set(paths), {'zipzi.values.' + name for name in v.__all__})
            self.assertTrue(files('zipzi').joinpath('docs/guide.md').is_file())
            for path in ('Session.missing', '_kernel', 'values.__dict__', 'values.'):
                with self.assertRaises(KeyError): zipzi.describe(path)
            for options in ({'limit': 0}, {'offset': -1}, {'limit': 65}):
                with self.assertRaises(ValueError): zipzi.describe(**options)

    def test_large_value_previews_preserve_complete_wire_content(self):
        value = v.ImageInputRgba(width=100, height=100, bytes=tuple(range(256)) * 100)
        self.assertLess(len(repr(value)), 300)
        self.assertEqual(len(value.to_wire()['bytes']), 25600)
        self.assertEqual(v.ImageInput.from_wire(value.to_wire()), value)
        identity = str(uuid4())
        reference = v.SceneReferenceResolved(object_id=identity)
        self.assertIn(identity, repr(reference))
        command = v.CommandRename(target=v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=identity), name='Angle')
        self.assertIn(identity, repr(command))

    def test_snapshot_records_decode_values_and_reject_non_object_queries(self):
        source = v.QuerySourcePublished(value=v.Version(session=str(uuid4()), generation=0, revision=0))
        snapshot = zipzi.Snapshot(source, 'content', {'records': (
            {'address': {'kind': 'parameter', 'id': 'p'},
             'value': v.ParameterCreateSpec(name='Angle', minimum=0, maximum=1, default_value=0).to_wire()},
        )})
        self.assertEqual(snapshot.records(v.ParameterCreateSpec)[0].name, 'Angle')
        with self.assertRaises(TypeError): zipzi.Snapshot(source, 'content', {}).records(v.Parameter)
        with self.assertRaises(TypeError): snapshot.records(v.Mesh)

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

    def test_metadata_read_modify_write_preserves_other_root_fields(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            part = str(uuid4())
            with session.draft() as draft:
                draft.apply(v.CommandCreatePart(value=v.Part(
                    id=part, runtime_id=part, name='owner', parent_id='', enabled=True, draw_order=0)))
                roots = draft.query(v.QueryMetadata()).value
                draft.apply(
                    v.CommandEditorState(value=v.EditorState(objects={part: v.ObjectEditorState(hidden_in_editor=True)})),
                    v.CommandDisplayInfo(value=replace(v.DisplayInfo.from_wire(roots['display_info']),
                        parameters=(), extensions={'vendor': {'keep': True}})),
                    v.CommandModelSettings(value=replace(v.Model3Settings.from_wire(roots['model3_settings']),
                        layout={'CenterX': 1., 'Width': 2.}, extensions={'vendor': 'keep'})),
                )
            published = session.query(v.QueryMetadata())
            with session.draft() as draft:
                snapshot = draft.query(v.QueryMetadata())
                self.assertEqual(snapshot.source, v.QuerySourceDraft(value=draft.source))
                editor = v.EditorState.from_wire(snapshot.value['editor_state'])
                display = v.DisplayInfo.from_wire(snapshot.value['display_info'])
                model = v.Model3Settings.from_wire(snapshot.value['model3_settings'])
                draft.apply(
                    v.CommandEditorState(value=replace(editor, objects={part: replace(editor.objects[part], locked=True)})),
                    v.CommandDisplayInfo(value=replace(display, parts=())),
                    v.CommandModelSettings(value=replace(model, layout={**model.layout, 'CenterX': 3.})),
                )
                after = draft.query(v.QueryMetadata())
                self.assertEqual(session.query(v.QueryMetadata()).value, published.value)
                with self.assertRaises(zipzi.SdkFailure) as stale:
                    session.query(v.QueryMetadata(), source=snapshot.source)
                self.assertEqual(stale.exception.code, 'STALE_SOURCE')
            result = session.query(v.QueryMetadata())
            self.assertEqual(result.value, after.value)
            editor = v.EditorState.from_wire(result.value['editor_state'])
            self.assertTrue(editor.objects[part].locked)
            self.assertTrue(editor.objects[part].hidden_in_editor)
            display = v.DisplayInfo.from_wire(result.value['display_info'])
            self.assertEqual(display.parameters, ())
            self.assertEqual(display.parts, ())
            self.assertEqual(display.extensions, {'vendor': {'keep': True}})
            model = v.Model3Settings.from_wire(result.value['model3_settings'])
            self.assertEqual(model.layout, {'CenterX': 3., 'Width': 2.})
            self.assertEqual(model.extensions, {'vendor': 'keep'})
            with self.assertRaises(TypeError):
                result.value['editor_state']['objects'][part]['locked'] = False
            with self.assertRaises(zipzi.SdkFailure) as stale:
                session.query(v.QueryMetadata(), source=published.source)
            self.assertEqual(stale.exception.code, 'STALE_SOURCE')

    def test_rejected_batch_admission_keeps_draft_usable_and_cancellable(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            for method in ('apply', 'apply_async'):
                with self.subTest(method=method):
                    draft = session.draft()
                    source = draft.source
                    with self.assertRaises(zipzi.SdkFailure) as rejected:
                        getattr(draft, method)(*([create()] * 4097))
                    self.assertEqual(rejected.exception.code, 'RESOURCE_LIMIT')
                    self.assertEqual(draft.state, 'ready')
                    self.assertEqual(draft.source, source)
                    draft.apply(create())
                    live_source = v.QuerySourceDraft(value=draft.source)
                    draft.cancel()
                    with self.assertRaises(zipzi.SdkFailure) as ended:
                        session.query(v.QuerySummary(), source=live_source)
                    self.assertEqual(ended.exception.code, 'DRAFT_ENDED')
            self.assertEqual(session.query(v.QuerySummary()).value['parameters'], 0)

    def test_incomplete_and_released_blob_admission_preserve_candidate(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            asset = str(uuid4())
            with session.draft() as draft, runtime.blob(4) as blob:
                draft.apply(v.CommandCreateAsset(id=asset, name='pixel'))
                source = draft.source
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    draft.replace_image_blob(asset, blob, width=1, height=1)
                self.assertEqual(rejected.exception.code, 'INCOMPLETE_BLOB')
                self.assertEqual(draft.state, 'ready')
                self.assertEqual(draft.source, source)
                blob.append(0, b'\xff\x00\x00\xff')
                blob.seal()
                draft.replace_image_blob(asset, blob, width=1, height=1)
                source = draft.source
                blob.close()
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    draft.replace_image_blob(asset, blob, width=1, height=1)
                self.assertEqual(rejected.exception.code, 'BLOB_NOT_FOUND')
                self.assertEqual(draft.state, 'ready')
                self.assertEqual(draft.source, source)
            with session.image_blob(asset) as blob:
                self.assertEqual(blob.read(0, 4), b'\xff\x00\x00\xff')

    def test_accepted_failure_ends_candidate_on_host_and_client(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            draft = session.draft()
            source = v.QuerySourceDraft(value=draft.source)
            id = str(uuid4())
            with self.assertRaises(zipzi.SdkFailure) as rejected:
                draft.apply(create(id), create(id))
            self.assertEqual(rejected.exception.details['draft_state'], 'ended')
            self.assertEqual(draft.state, 'ended')
            draft.cancel()
            with self.assertRaises(zipzi.SdkFailure) as ended:
                session.query(v.QuerySummary(), source=source)
            self.assertEqual(ended.exception.code, 'DRAFT_ENDED')
            self.assertEqual(session.query(v.QuerySummary()).value['parameters'], 0)

    def test_draft_operations_observe_candidates_ended_by_detach(self):
        with self.factory() as runtime:
            session = runtime.sessions.create()
            candidates = [session.draft() for _ in range(3)]
            session.detach()
            operations = (
                lambda draft: draft.preview(create()),
                lambda draft: draft.query(v.QuerySummary()),
                lambda draft: draft.cancel(),
            )
            for draft, operation in zip(candidates, operations):
                with self.subTest(operation=operation):
                    with self.assertRaises(zipzi.SdkFailure) as ended:
                        operation(draft)
                    self.assertEqual(ended.exception.code, 'DRAFT_ENDED')
                    self.assertEqual(draft.state, 'ended')
                    draft.cancel()

    def test_motion_recipe_samples_at_reported_playback_local_time(self):
        from zipzi.docs import recipes
        with self.factory() as runtime:
            session = runtime.sessions.create()
            parameter, _, _ = recipes.create_scene(session)
            clip = recipes.create_linear_motion(session, parameter)
            before = session.info()
            times = (0., .25, .5, .75, 1.)
            report = recipes.sample_animation(session, times, motions=((clip, 0.),))
            for requested, frame in zip(times, report['frames']):
                animation = frame['animation']
                position, = animation['motion_positions']
                expected_local = max(0., requested - 1. / 60.)
                self.assertAlmostEqual(position['local_time'], expected_local, places=5)
                self.assertAlmostEqual(animation['parameters'][parameter], -20. + 40. * expected_local, places=4)
            after = session.info()
            self.assertEqual(after['version'], before['version'])
            self.assertEqual(after['history'], before['history'])
            parameter_value, = session.query(v.QueryObjects(addresses=(
                v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter),
            ))).records(v.Parameter)
            self.assertEqual(parameter_value.default_value, 0)

    def test_installed_recipes_complete_authoring_pagination_sampling_and_save(self):
        from zipzi.docs import recipes
        with self.factory() as runtime, tempfile.TemporaryDirectory(prefix='zipzi-recipes-') as directory:
            session = runtime.sessions.create()
            parameter, mesh, asset = recipes.create_scene(session)
            with session.draft('More parameters') as draft:
                draft.apply(create(), create())
            self.assertEqual(len(recipes.list_parameters(session, page_size=1)), 3)
            updated = recipes.rename_parameter(session, parameter, 'Head angle')
            self.assertEqual(updated.name, 'Head angle')
            history = session.info()['history']
            self.assertEqual(history['undo_steps'], 3)
            self.assertEqual(len(recipes.sample_animation(session, (0., .1))['frames']), 2)
            self.assertEqual(session.info()['history'], history)
            with session.image_blob(asset) as image:
                self.assertEqual(recipes.read_all(image), bytes((255, 0, 0, 255)))
            path = Path(directory) / 'recipe.zipzi'
            with session.save(path) as job:
                receipt = job.wait(timeout=10)
            self.assertTrue(receipt['published'])
            self.assertIsInstance(receipt['durable'], bool)
            self.assertIsInstance(receipt['warnings'], tuple)
            with session.open(path) as job: job.wait(timeout=10)
            selection = v.QueryObjects(addresses=(v.ObjectAddress(kind=v.ObjectKind.MESH, id=mesh),))
            self.assertEqual(session.query(selection).records(v.Mesh)[0].name, 'Triangle')

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
            from zipzi.docs.recipes import observation_packets
            packets = observation_packets(payload)
            self.assertEqual(packets[0][0]['source'], source.to_wire())
            self.assertTrue(packets[0][1].startswith(b'\x89PNG\r\n\x1a\n'))
            with self.assertRaises(ValueError): observation_packets(payload[:-1])

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


class LocalLifecycle(Lifecycle,unittest.TestCase):
    def test_documented_first_local_run_saves_a_reopenable_project(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder).resolve() / 'first.zipzi'
            output = documented_example('first-local', [str(path)])
            self.assertIn('Angle', output)
            self.assertIn(str(path), output)
            with zipzi.local() as runtime:
                session = runtime.sessions.create()
                with session.open(path) as job:
                    job.wait()
                parameter, = session.query(v.QueryCollection(kind=v.ObjectKind.PARAMETER, offset=0, limit=128)).records(v.Parameter)
                self.assertEqual((parameter.name, parameter.minimum, parameter.maximum, parameter.default_value), ('Angle', -30, 30, 0))


class ConnectedLifecycle(Lifecycle,unittest.TestCase):
    factory=staticmethod(connected)

    def test_documented_first_connected_run_selects_without_editing(self):
        with connected() as runtime:
            session = runtime.sessions.create()
            decoy = runtime.sessions.create()
            before = session.info()
            # The fixture owns the explicit endpoint; the documentation script
            # creates its own real client and attaches by the printed session ID.
            arguments = ['--endpoint', str(runtime._backend._endpoint)]
            listing = documented_example('first-connected', arguments)
            self.assertIn(session.id, listing)
            self.assertIn(decoy.id, listing)
            output = documented_example('first-connected', [*arguments, '--session', session.id])
            self.assertIn(session.id, output)
            self.assertIn(before['document_id'], output)
            self.assertEqual(session.info(), before)


class Recovery(unittest.TestCase):
    def test_recovered_rejection_retains_authoritative_draft_state(self):
        for oversized in (True, False):
            with self.subTest(oversized=oversized), connected('batch') as runtime:
                session = runtime.sessions.create()
                draft = session.draft()
                command = create(str(uuid4()))
                commands = [command] * (4097 if oversized else 2)
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    draft.apply(*commands)
                with self.assertRaises(zipzi.SdkFailure) as recovered:
                    zipzi.recover_call(lost.exception)
                state = 'ready' if oversized else 'ended'
                self.assertEqual(recovered.exception.details['draft_state'], state)
                self.assertEqual(draft.state, state)
                if oversized:
                    draft.apply(create())
                    draft.publish()
                else:
                    draft.cancel()

    def test_recovered_job_release_is_idempotent_and_context_safe(self):
        with connected('release_job') as runtime:
            session = runtime.sessions.create()
            with session.draft() as draft:
                with draft.apply_async(create()) as job:
                    job.wait(timeout=5)
                    with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                        job.close()
                    self.assertIsNone(zipzi.recover_call(lost.exception))
                    job.close()
                self.assertEqual(draft.state, 'ready')
            self.assertEqual(session.query(v.QuerySummary()).value['parameters'], 1)

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
