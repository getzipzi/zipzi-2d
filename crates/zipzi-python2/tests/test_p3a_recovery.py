"""P3A-1 independent call, candidate and accepted-job contracts."""
from pathlib import Path
import sys
import threading
import time
import typing
import weakref
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_api as support

zipzi, meshes, parameters, scene = support.zipzi, support.meshes, support.parameters, support.scene


class P3ARecovery(unittest.TestCase):
    setUp = support.PythonSdk.setUp
    runtime = support.PythonSdk.runtime
    control = support.PythonSdk.control

    def test_typed_helper_executes_public_write_once_and_retains_complete_projection(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("typed create")
        with patch.object(runtime._backend, "apply", wraps=runtime._backend.apply) as sent:
            id = zipzi.recover_call(edit.meshes.create, meshes.MeshSpec("Once", support.GEOMETRY))
        self.assertIsInstance(id, str)
        self.assertEqual(sent.call_count, 1)
        self.assertEqual(edit.meshes.get(id).source.sequence, 1)
        self.assertEqual(edit.state, zipzi.EditState.editing)
        edit.commit().wait()
        self.assertEqual(edit.state, zipzi.EditState.committed)
        self.assertTrue(edit.receipt.changed)
        self.assertEqual(session.meshes.get(id).value.name, "Once")
        self.assertEqual(session.info().history, (1, 0))

    def test_helper_rejects_business_functions_wrappers_and_unbound_methods_before_execution(self):
        side_effects = []
        runtime = self.runtime("local")
        edit = runtime.sessions.create(support.uid(1)).edit("not invoked")
        class Business:
            def create(self):
                side_effects.append("called")
        for function in (lambda: side_effects.append("called"), Business().create, type(edit).commit):
            with self.subTest(function=function):
                with self.assertRaises(TypeError):
                    zipzi.recover_call(function)
        self.assertEqual(side_effects, [])
        self.assertEqual(edit.state, zipzi.EditState.editing)
        edit.cancel()
        self.assertFalse(hasattr(edit, "recover"))

    def test_helper_stops_after_one_recovery_and_unknown_remains_resumable(self):
        from zipzi.backends import connected
        runtime = self.runtime("connected")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("two losses")
        exchange = connected._exchange
        def lose(stream, body):
            exchange(stream, body)
            raise EOFError("drop real reply")
        with patch.object(runtime._backend, "apply", wraps=runtime._backend.apply) as writes:
            with patch.object(connected, "_exchange", side_effect=lose) as attempts:
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    zipzi.recover_call(edit.meshes.create, meshes.MeshSpec("Once", support.GEOMETRY))
            self.assertEqual(writes.call_count, 1)
            self.assertEqual(attempts.call_count, 2)  # write, one recovery handshake
            id = lost.exception.recover()
        self.assertIsInstance(id, str)
        self.assertEqual(edit.meshes.get(id).source.sequence, 1)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_commit_recovery_never_waits_for_queued_publication(self):
        runtime = self.runtime("connected", drop="commit")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("queued commit")
        id = edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
        self.control(runtime, "pause_dispatch")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.commit()
        with patch.object(zipzi.Job, "wait", side_effect=AssertionError("Recovery waited")):
            job = lost.exception.recover()
        self.assertIsInstance(job, zipzi.Job)
        self.assertEqual(edit.state, zipzi.EditState.committing)
        self.assertIsNone(edit.receipt)
        self.assertEqual(job.state(), "queued")
        self.control(runtime, "resume_dispatch")
        receipt = job.wait()
        self.assertIs(edit.receipt, receipt)
        self.assertEqual(session.meshes.get(id).value.name, "Face")
        self.assertEqual(session.info().history, (1, 0))

    def test_caught_wait_or_cancel_unknown_cannot_leave_context_as_success(self):
        for operation in ("job", "release_edit"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                session = runtime.sessions.create(support.uid(1))
                original = None
                with self.assertRaises(zipzi.OutcomeUnknown) as escaped:
                    with session.edit(operation) as edit:
                        edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
                        try:
                            if operation == "job":
                                edit.commit().wait()
                            else:
                                edit.cancel()
                        except zipzi.OutcomeUnknown as error:
                            original = error
                self.assertIs(escaped.exception, original)
                self.assertEqual(edit.state, zipzi.EditState.unknown)
                self.assertIsNone(edit.receipt)
                result = escaped.exception.recover()
                if operation == "job":
                    self.assertIs(result, edit.receipt)
                    self.assertEqual(edit.state, zipzi.EditState.committed)
                    self.assertEqual(session.info().history, (1, 0))
                else:
                    self.assertIsNone(result)
                    self.assertEqual(edit.state, zipzi.EditState.cancelled)
                    self.assertIsNone(edit.receipt)
                    self.assertEqual(session.info().history, (0, 0))

    def test_explicit_cancel_and_cached_edit_observation_have_no_publication_or_rpc(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(support.uid(1))
                with session.edit("cancel") as edit:
                    edit.meshes.create(meshes.MeshSpec("Discard", support.GEOMETRY))
                    edit.cancel()
                with patch.object(runtime._backend, "info", side_effect=AssertionError("Property made an RPC")):
                    self.assertEqual(edit.state, zipzi.EditState.cancelled)
                    self.assertIsNone(edit.receipt)
                self.assertEqual(session.info().history, (0, 0))

    def test_timeout_is_known_pending_not_failure_cancel_or_unknown(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(support.uid(1))
                self.control(runtime, "pause_dispatch")
                job = session.project.save(self.folder / mode)
                with self.assertRaises(zipzi.JobTimeout) as timed_out:
                    zipzi.recover_call(job.wait, timeout=0.)
                self.assertNotIsInstance(timed_out.exception, zipzi.OutcomeUnknown)
                self.assertEqual(timed_out.exception.recovery_action, "wait_job")
                self.assertIsNone(job.pending_request)
                self.assertEqual(job.state(), "queued")
                self.control(runtime, "resume_dispatch")
                saved = job.wait()
                self.assertTrue(saved.durable)
                self.assertIs(job.wait(timeout=0.), saved)
                self.assertEqual(job.state(), "succeeded")

    def test_none_waits_beyond_connection_poll_timeout_and_still_releases_gil(self):
        runtime = self.runtime("connected")
        session = runtime.sessions.create(support.uid(1))
        self.control(runtime, "pause_dispatch")
        job = session.project.save(self.folder / "indefinite")
        runtime._backend._timeout = .001
        def resume():
            time.sleep(.03)
            self.control(runtime, "resume_dispatch")
        worker = threading.Thread(target=resume)
        worker.start()
        try:
            self.assertTrue(job.wait(timeout=None).durable)
        finally:
            worker.join(timeout=5)
        self.assertFalse(worker.is_alive())

    def test_job_close_recovery_returns_none_and_caches_original_result(self):
        runtime = self.runtime("connected", drop="job")
        session = runtime.sessions.create(support.uid(1))
        job = session.project.save(self.folder / "saved")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            job.close()
        self.assertIsNone(lost.exception.recover())
        self.assertTrue(job.wait().durable)
        request = runtime._backend._next
        job.close()
        self.assertEqual(runtime._backend._next, request)
        with self.assertRaises(zipzi.SdkFailure) as resolved:
            lost.exception.recover()
        self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")

    def test_helper_recovers_multistage_buffers_without_replaying_method(self):
        content = b"immutable" * (1 << 18)
        for operation in ("create_blob", "append_blob_binary", "seal_blob"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                with zipzi.recover_call(runtime.resources.prepare_buffer, content) as buffer:
                    self.assertEqual(buffer.read(), content)
                with self.assertRaises(zipzi.SdkFailure):
                    buffer.read()
                # A bounded binary reply cache retains this single allocation
                # until protocol expiry, independently of the closed handle.
                usage = runtime.resources.usage()
                self.assertEqual(usage.blob_bytes, len(content))
                self.assertEqual(usage.preparing_bytes, 0)

    def test_confirmed_busy_and_domain_failure_have_distinct_guidance_without_retry(self):
        runtime = self.runtime("connected", settings={"ZIPZI_TEST_BUSY_COMMIT_ONCE": "1"})
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("busy")
        id = edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
        with self.assertRaises(zipzi.Busy) as refused:
            zipzi.recover_call(edit.commit)
        self.assertEqual(refused.exception.recovery_action, "retry")
        self.assertEqual(edit.state, zipzi.EditState.editing)
        self.assertEqual(session.info().history, (0, 0))
        zipzi.recover_call(edit.commit).wait()
        self.assertEqual(session.meshes.get(id).value.name, "Face")
        failed = session.edit("ordinary failure")
        with self.assertRaises(zipzi.SdkFailure) as domain:
            zipzi.recover_call(failed.parameters.create, parameters.ParameterSpec("Bad", 1., -1., 0.))
        self.assertEqual(domain.exception.recovery_action, "restart_edit")
        self.assertEqual(failed.state, zipzi.EditState.aborted)
        self.assertIsNone(domain.exception.requested)
        self.assertIsNone(domain.exception.allowed)
        self.assertEqual(session.info().history, (1, 0))

    def test_runtime_close_requires_explicit_abandon_and_never_replays_write(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("unknown")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.meshes.create(meshes.MeshSpec("Once", support.GEOMETRY))
        request = runtime._backend._next
        with self.assertRaises(zipzi.Busy):
            runtime.close()
        self.assertEqual(runtime._backend._next, request)
        runtime.close(abandon_pending=True)
        with self.assertRaises(zipzi.SdkFailure):
            lost.exception.recover()
        self.assertEqual(runtime._backend._next, request)

    def test_error_and_helper_annotations_resolve_offline(self):
        for method in (zipzi.OutcomeUnknown.recover, zipzi.SdkFailure.__init__, zipzi.recover_call,
                       zipzi.Job.wait, zipzi.Edit.commit, zipzi.Edit.state.fget, zipzi.Edit.receipt.fget):
            self.assertTrue(typing.get_type_hints(method))

    def test_resolved_call_releases_owned_python_inputs_while_edit_remains_alive(self):
        runtime = self.runtime("local")
        session = runtime.sessions.create(support.uid(1))
        edit = session.edit("release inputs")
        spec = meshes.MeshSpec("Face", support.GEOMETRY)
        reference = weakref.ref(spec)
        id = edit.meshes.create(spec)
        del spec
        self.assertIsNone(reference())
        self.assertEqual(edit.meshes.get(id).value.name, "Face")
        edit.commit().wait()

    def test_conversion_inside_generated_write_cannot_be_caught_and_committed(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(support.uid(1))
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    with session.edit("conversion failure") as edit:
                        id = edit.meshes.create(meshes.MeshSpec("Unpublished", support.GEOMETRY))
                        try:
                            edit.meshes.set_positions(id, vertex_ids=(42, 7, 99), positions=None)
                        except TypeError:
                            pass
                self.assertEqual(rejected.exception.code, "EDIT_ABORTED")
                self.assertEqual(edit.state, zipzi.EditState.aborted)
                self.assertEqual(session.info().history, (0, 0))

    def test_caught_read_or_independent_preparation_unknown_preserves_its_original_owner(self):
        for operation in ("mesh_snapshot", "prepare_image"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                session = runtime.sessions.create(support.uid(1))
                original = None
                with self.assertRaises(zipzi.OutcomeUnknown) as escaped:
                    with session.edit("read/preparation loss") as edit:
                        id = edit.meshes.create(meshes.MeshSpec("Face", support.GEOMETRY))
                        try:
                            if operation == "mesh_snapshot":
                                edit.meshes.get(id)
                            else:
                                runtime.resources.prepare_image(1, 1, b"rgba")
                        except zipzi.OutcomeUnknown as error:
                            original = error
                self.assertIs(escaped.exception, original)
                self.assertEqual(edit.state, zipzi.EditState.editing)
                result = escaped.exception.recover()
                if operation == "mesh_snapshot":
                    self.assertIsInstance(result, zipzi.Snapshot)
                    self.assertEqual(result.value.name, "Face")
                else:
                    self.assertIsInstance(result, zipzi.PreparedImage)
                    self.assertEqual(result.pixels().rgba, b"rgba")
                    result.close()
                self.assertEqual(edit.state, zipzi.EditState.editing)
                edit.commit().wait()
                self.assertEqual(session.info().history, (1, 0))


if __name__ == "__main__":
    unittest.main()
