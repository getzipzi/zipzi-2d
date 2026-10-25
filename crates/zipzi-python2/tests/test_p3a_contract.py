"""P3A target-contract regressions, independent of generated expected values.

These are deliberately strict tests, not expected failures. Run with the P3A
baseline command while implementing the contract; a failing report is evidence
of a gap, never evidence that P3A has passed. Native/host fixtures are the same
real Local/Connected fixtures used by the existing numerical behavior suite.
"""
import typing
import unittest

import test_api as support

zipzi = support.zipzi
meshes, parameters, bindings, scene = (
    support.meshes, support.parameters, support.bindings, support.scene)


class P3AContract(unittest.TestCase):
    setUp = support.PythonSdk.setUp
    runtime = support.PythonSdk.runtime

    def test_mesh_create_recovery_returns_id_once(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("recover mesh create")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
        result = lost.exception.recover()
        self.assertIsInstance(result, str)
        self.assertEqual(edit.meshes.get(result).source.sequence, 1)
        self.assertEqual(edit.meshes.get(result).value.name, "Face")
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))
        with self.assertRaises(zipzi.SdkFailure) as stale:
            lost.exception.recover()
        self.assertEqual(stale.exception.code, "NO_PENDING_REQUEST")

    def test_parameter_create_recovery_returns_id(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("recover parameter create")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.parameters.create(parameters.ParameterSpec("Angle", -1., 1., 0.))
        result = lost.exception.recover()
        self.assertIsInstance(result, str)
        self.assertEqual(edit.parameters.get(result).value.name, "Angle")
        self.assertEqual(edit.parameters.get(result).source.sequence, 1)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_image_create_recovery_returns_id(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("recover image create")
        with runtime.resources.prepare_image(1, 1, b"rgba") as image:
            with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                edit.resources.create_image("Texture", image)
            result = lost.exception.recover()
            self.assertIsInstance(result, str)
            self.assertEqual(edit.resources.get(result).value.pixels.rgba, b"rgba")
            self.assertEqual(edit.resources.get(result).source.sequence, 1)
            edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_binding_create_recovery_returns_id(self):
        runtime = self.runtime("connected", drop="apply_authoring", drop_after=2)
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("recover binding create")
        face = edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
        angle = edit.parameters.create(parameters.ParameterSpec("Angle", 0., 1., 0.))
        spec = bindings.MeshBindingSpec(face, (bindings.Axis(angle, (0., 1.)),), (
            bindings.Keyform((0.,), support.GEOMETRY.positions),
            bindings.Keyform((1.,), ((2., 0.), (3., 0.), (2., 1.)))))
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.bindings.create(spec)
        result = lost.exception.recover()
        self.assertIsInstance(result, str)
        self.assertEqual(edit.bindings.get(result).source.sequence, 3)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_scene_get_recovery_keeps_single_snapshot(self):
        runtime = self.runtime("connected", drop="read_selection")
        session = runtime.sessions.create(support.uid(1))
        with session.edit("create part") as edit:
            part = edit.apply(scene.CreatePart(scene.PartSpec("Face"))).ids[0]
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            session.scene.get(part)
        recovered = lost.exception.recover()
        ordinary = session.scene.get(part)
        self.assertIs(type(recovered), type(ordinary))
        self.assertEqual(recovered, ordinary)
        self.assertEqual(recovered.value.name, "Face")
        self.assertEqual(session.info().history, (1, 0))

    def test_commit_recovery_returns_job_without_waiting(self):
        runtime = self.runtime("connected", drop="commit")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("recover commit")
        edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.commit()
        # A completed Job must still be returned as a Job. Returning Receipt
        # hides the wait obligation and changes the public call's type.
        job = lost.exception.recover()
        self.assertIsInstance(job, zipzi.Job)
        receipt = job.wait()
        self.assertIsInstance(receipt, zipzi.Receipt)
        self.assertTrue(receipt.changed)
        self.assertEqual(session.info().history, (1, 0))

    def test_caught_unknown_cannot_exit_edit_successfully(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        original = None
        with self.assertRaises(zipzi.OutcomeUnknown) as escaped:
            with session.edit("caught unknown") as edit:
                try:
                    edit.apply(meshes.Create(meshes.MeshSpec("Face", support.GEOMETRY)))
                except zipzi.OutcomeUnknown as error:
                    original = error
        self.assertIsNotNone(original)
        self.assertEqual(escaped.exception.pending, original.pending)
        result = escaped.exception.recover()
        self.assertIsInstance(result, zipzi.WriteResult)
        self.assertEqual(result.source.sequence, 1)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_recovery_annotation_is_runtime_resolvable(self):
        self.assertIs(typing.get_type_hints(zipzi.OutcomeUnknown.recover)["return"], object)

    def test_257_objects_have_complete_source_bound_pages(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(support.uid(1))
                with session.edit("257 named meshes") as edit:
                    ids = tuple(edit.meshes.create(meshes.MeshSpec(
                        "Repeated", support.GEOMETRY)) for _ in range(257))
                cursor, source, found = None, None, []
                for _ in range(258):
                    page = session.meshes.list(name="Repeated", limit=128, cursor=cursor)
                    if source is None:
                        source = page.source
                    self.assertEqual(page.source, source)
                    self.assertLessEqual(len(page.items), 128)
                    found.extend(item.id for item in page.items)
                    if page.next_cursor is None:
                        break
                    self.assertNotEqual(page.next_cursor, cursor)
                    cursor = page.next_cursor
                else:
                    self.fail("Pagination made no bounded progress")
                self.assertEqual(tuple(found), ids)
                self.assertEqual(session.info().history, (1, 0))

    def test_40_sessions_close_and_release_registry_slots(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                for n in range(40):
                    session = runtime.sessions.create(support.uid(n + 1))
                    closed = session.close()
                    self.assertIsInstance(closed, zipzi.SessionInfo)
                    self.assertEqual(closed.id, session.id)
                    self.assertEqual(session.close(), closed)
                    self.assertEqual(runtime.sessions.list(), ())

    def test_large_geometry_repr_is_bounded_without_losing_data(self):
        geometry = meshes.Geometry(tuple(range(1000)), ((0., 0.),) * 1000,
                                   ((0., 0.),) * 1000, ((0, 1, 2),))
        self.assertLessEqual(len(repr(geometry)), 1024)
        self.assertEqual(len(geometry.vertex_ids), 1000)
        self.assertEqual(geometry.positions[-1], (0., 0.))


if __name__ == "__main__":
    unittest.main()
