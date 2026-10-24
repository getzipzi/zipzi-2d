"""Installed public facade acceptance against real local host subprocesses.

Run through tools/python/test_connected.py (no source PYTHONPATH or native extension).
"""
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from uuid import uuid4
import zipzi
from zipzi._connection import _exchange, CONTRACT

HOST = Path(os.environ["ZIPZI_TEST_HOST"]).resolve()


class Connected(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zzr3-", dir="/tmp")
        self.directory = Path(self.temp.name)
        self.env = dict(os.environ)
        os.environ["ZIPZI_RUNTIME_DIR"] = str(self.directory)
        os.environ["ZIPZI_RUNTIME_PROFILE"] = "test"
        os.environ["ZIPZI_TEST_PID_LOG"] = str(self.directory / "pids")
        self.connections = []
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=5)
            for stream in (child.stdout, child.stderr):
                if stream is not None:
                    stream.close()
        for connection in self.connections:
            connection.close()
        pid_file = self.directory / "pids"
        if pid_file.exists():
            for pid in {int(p) for p in pid_file.read_text().splitlines()}:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    os.waitpid(pid, 0)
                except ChildProcessError:
                    pass
        os.environ.clear()
        os.environ.update(self.env)
        self.temp.cleanup()

    def connect(self, **kwargs):
        c = zipzi.connect(host=HOST, **kwargs)
        self.connections.append(c)
        return c

    def assert_code(self, code, action):
        with self.assertRaises((zipzi.ConnectionFailure, zipzi.SdkFailure)) as error:
            action()
        self.assertEqual(error.exception.code, code)

    def test_r4_modeling_recipe_and_stateless_candidate_samples(self):
        result = subprocess.run([sys.executable, "-I", str(Path(__file__).with_name("modeling_recipe.py")), "connected"], env=os.environ, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_r4_hundred_cancelled_images_reclaim_underlying_payloads(self):
        c = self.connect()
        session = c.create_session(str(uuid4()))
        initial = session.version
        for n in range(100):
            edit = session.edit("cancel image")
            edit.create_rgba_image(str(uuid4()), "draft", 1, 1, bytes([n, 0, 0, 255]))
            self.assertEqual(c.statistics()["image_memory"]["rgba_bytes"], 4)
            edit.cancel()
            stats = c.statistics()
            self.assertEqual(stats["memory_image_entries"], 0)
            self.assertEqual(stats["image_memory"]["rgba_bytes"], 0)
            self.assertEqual(stats["image_memory"]["preparing_bytes"], 0)
        self.assertEqual(session.version, initial)
        self.assertEqual(session.history_lengths(), (0, 0))

    def test_r4_chunked_blob_pixels_remain_owned_until_last_candidate_releases(self):
        c = self.connect()
        session = c.create_session(str(uuid4()))
        data = bytes([4, 3, 2, 255]) * (1024 * 257)
        with c.create_blob(len(data)) as blob:
            for offset in range(0, len(data), 1 << 20):
                blob.append(data[offset:offset + (1 << 20)])
            blob.seal()
            reconstructed = b"".join(blob.read(offset, min(1 << 20, len(data) - offset)) for offset in range(0, len(data), 1 << 20))
            self.assertEqual(reconstructed, data)
            self.assertEqual(c.statistics()["image_memory"]["blob_bytes"], len(data))
            edit = session.edit("blob image")
            edit.create_rgba_image_from_blob(blob, str(uuid4()), "chunked", 1024, 257)
            self.assertEqual(c.statistics()["image_memory"]["rgba_bytes"], len(data))
        self.assertEqual(c.statistics()["blobs"], 0)
        self.assertEqual(c.statistics()["image_memory"]["rgba_bytes"], len(data))
        edit.cancel()
        self.assertEqual(c.statistics()["image_memory"]["rgba_bytes"], 0)

    def test_r4_blob_append_response_loss_recovers_offset_without_duplicate_bytes(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "append_blob"
        c = self.connect()
        with c.create_blob(4) as blob:
            with self.assertRaises(zipzi.OutcomeUnknown):
                blob.append(bytes([1, 2, 3, 255]))
            self.assertEqual(blob.offset, 0)
            self.assertEqual(blob.recover(), 4)
            self.assertEqual(blob.offset, 4)
            blob.seal()
            session = c.create_session(str(uuid4()))
            edit = session.edit("recovered upload")
            edit.create_rgba_image_from_blob(blob, str(uuid4()), "image", 1, 1)
            edit.cancel()
        self.assertEqual(c.statistics()["blobs"], 0)
        self.assertEqual(c.statistics()["image_memory"]["rgba_bytes"], 0)

    def test_r4_plan_source_sequence_and_client_ownership(self):
        c = self.connect()
        session = c.create_session(str(uuid4()))
        resource = str(uuid4())
        with session.edit("resource") as edit:
            edit.create_rgba_image(resource, "image", 1, 1, bytes([1, 2, 3, 255]))
        plan = session.plan_modeling("resource", {"operation": "patch_name", "id": resource, "expected": "image", "name": "renamed"})
        other = self.connect()
        edit = other.attach_session(session._native.session_id).edit("foreign plan")
        self.assert_code("WRONG_OWNER", lambda: edit.apply_modeling_plan(plan))
        edit.cancel()
        edit = session.edit("candidate plan")
        candidate_plan = edit.plan_modeling("resource", {"operation": "patch_name", "id": resource, "expected": "image", "name": "renamed"})
        edit.apply_modeling({"op": "resource", "intent": {"operation": "patch_name", "id": resource, "expected": "image", "name": "changed"}})
        self.assert_code("STALE_PLAN", lambda: edit.apply_modeling_plan(candidate_plan))
        edit.cancel()
        candidate_plan.close()
        with session.edit("apply formal plan") as edit:
            edit.apply_modeling_plan(plan)
        plan.close()
        self.assertEqual(c.statistics()["plans"], 0)
        self.assertEqual(session.modeling_query(assets=[resource])["records"]["assets"][0]["name"], "renamed")

    def test_installed_without_native_shared_history_mesh_parameter_save_events(self):
        self.assertIsNone(importlib.util.find_spec("zipzi._native"))
        self.assertNotIn("zipzi-python/python", zipzi.__file__)
        c = self.connect()
        a = c.create_session(str(uuid4()))
        sid = a._native.session_id
        other = self.connect()
        b = other.attach_session(sid)
        subscription = a._native.subscribe()
        parameter, mesh = str(uuid4()), str(uuid4())
        with a.edit("agent creates") as edit:
            edit.create_parameter(parameter, "Angle", -1, 1, 0)
            edit.create_mesh(zipzi.MeshRecordSpec(mesh, "Triangle",
                zipzi.MeshGeometryData([0,1,2], [(0,0),(1,0),(0,1)], [(0,0),(1,0),(0,1)], [(0,1,2)]), zipzi.MeshDrawingData("")))
            self.assertEqual(edit.parameter(parameter).name, "Angle")
            self.assertEqual(edit.mesh(mesh).name, "Triangle")
        with b.edit("other client") as edit:
            edit.rename_mesh(mesh, "Face")
            edit.update_positions(mesh, [0], [(3.25, -1.5)])
            edit.replace_parameter(parameter, "Yaw", -2, 2, .5)
        self.assertEqual(a.mesh(mesh).positions[0], (3.25,-1.5))
        self.assertEqual(a.parameter(parameter).name, "Yaw")
        self.assertEqual(a.history_lengths(), (2,0))
        b.undo()
        with a.edit("noop"):
            pass
        self.assertEqual(a.history_lengths(), (1,1))
        b.redo()
        self.assertEqual(a.mesh(mesh).name, "Face")
        events = c.request({"op": "events", "cursor": subscription["cursor"]})
        self.assertTrue(any(e["kind"] == "published" for e in events))
        batch = a.query_objects(meshes=[mesh], parameters=[parameter])
        self.assertEqual(batch.meshes[0].id, mesh)
        saved = a.save(self.directory / "project")
        self.assertTrue(saved.manifest.is_file())
        self.assertFalse(a.modified)
        reopened = other.open_project(saved.manifest)
        self.assertEqual(reopened._native.session_id, sid)
        self.assert_code("UNSUPPORTED_OPERATION", lambda: a.evaluate({}))

    def test_ordinary_and_local_errors_poison_candidate_cleanup_preserves_original(self):
        c = self.connect()
        a = c.create_session(str(uuid4()))
        parameter = str(uuid4())
        with a.edit("seed") as edit:
            edit.create_parameter(parameter, "A", -1, 1, 0)
        edit = a.edit("bad")
        self.assert_code("MISSING_MESH", lambda: edit.rename_mesh(str(uuid4()), "absent"))
        self.assertEqual(edit.state, "aborted")
        self.assert_code("EDIT_ABORTED", edit.commit)
        self.assertEqual(a.history_lengths(), (1,0))
        with self.assertRaises(ValueError):
            with a.edit("bad conversion") as edit:
                try:
                    edit.create_parameter(str(uuid4()), "bad", float("nan"), 1, 0)
                except ValueError:
                    self.assertEqual(edit.state, "aborted")
                    raise
        # Release transport failure must not replace the body exception.
        with self.assertRaisesRegex(RuntimeError, "body wins"):
            with a.edit("cleanup") as edit:
                os.kill(c.host_pid, signal.SIGTERM)
                raise RuntimeError("body wins")

    def test_busy_keeps_edit_and_new_request_retry(self):
        os.environ["ZIPZI_TEST_BUSY_COMMIT_ONCE"] = "1"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        edit = a.edit("retry")
        parameter = str(uuid4())
        edit.create_parameter(parameter, "A", -1, 1, 0)
        with self.assertRaises(zipzi.Busy):
            with edit:
                edit.commit()
        self.assertEqual(edit.state, "editing")
        self.assertEqual(edit.parameter(parameter).name, "A")
        edit.commit()
        self.assertEqual(edit.state, "committed")
        self.assertEqual(a.history_lengths(), (1,0))

    def test_admission_budget_keeps_edit_available_after_context_exit(self):
        os.environ["ZIPZI_TEST_JOB_COUNT"] = "0"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        parameter = str(uuid4())
        with self.assertRaises(zipzi.ConnectionFailure) as failure:
            with a.edit("admission budget") as edit:
                edit.create_parameter(parameter, "A", -1,1,0)
        self.assertEqual(failure.exception.code, "BUDGET_EXCEEDED")
        self.assertEqual(edit.state, "editing")
        self.assertEqual(edit.parameter(parameter).name, "A")
        self.assertEqual(a.history_lengths(), (0,0))
        edit.cancel()

    def test_lost_busy_response_restores_editing_on_recovery(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "commit"
        os.environ["ZIPZI_TEST_BUSY_COMMIT_ONCE"] = "1"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        edit = a.edit("lost Busy")
        edit.create_parameter(str(uuid4()), "A", -1,1,0)
        self.assert_code("OUTCOME_UNKNOWN", edit.commit)
        self.assert_code("BUSY", edit.recover)
        self.assertEqual(edit.state, "editing")
        edit.commit()
        self.assertEqual(a.history_lengths(), (1,0))

    def test_apply_response_lost_context_does_not_cancel_and_same_id_recovers(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "apply"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        parameter = str(uuid4())
        with self.assertRaises(zipzi.OutcomeUnknown):
            with a.edit("lost apply") as edit:
                edit.create_parameter(parameter, "A", -1, 1, 0)
        request_id = edit.pending_request["request_id"]
        self.assertEqual(edit.state, "outcome_unknown")
        self.assertEqual(c.request_status(request_id)["state"], "completed")
        self.assert_code("OUTCOME_UNKNOWN", edit.commit)
        self.assert_code("OUTCOME_UNKNOWN", edit.cancel)
        self.assertEqual(edit.pending_request["request_id"], request_id)
        edit.recover()
        self.assertEqual(edit.state, "editing")
        self.assertEqual(edit.parameter(parameter).name, "A")
        edit.commit()
        self.assertEqual(a.history_lengths(), (1,0))

    def test_commit_response_lost_context_recovers_exactly_once(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "commit"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        with self.assertRaises(zipzi.OutcomeUnknown):
            with a.edit("lost commit") as edit:
                edit.create_parameter(str(uuid4()), "A", -1, 1, 0)
        self.assertEqual(edit.state, "outcome_unknown")
        edit.recover()
        self.assertEqual(edit.state, "committed")
        self.assertEqual(a.history_lengths(), (1,0))
        edit.__exit__(None, None, None)
        self.assertEqual(a.history_lengths(), (1,0))

    def test_terminal_job_response_lost_can_be_recovered_after_job_release(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "job"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        edit = a.edit("lost terminal")
        edit.create_parameter(str(uuid4()), "A", -1, 1, 0)
        self.assert_code("OUTCOME_UNKNOWN", edit.commit)
        edit.recover()
        self.assertEqual(a.history_lengths(), (1,0))

    def test_pending_commit_cannot_be_recovered_by_another_edit(self):
        for dropped in ("commit", "job"):
            with self.subTest(dropped=dropped):
                os.environ["ZIPZI_TEST_DROP_RESPONSE"] = dropped
                # Each subtest needs a fresh host to arm response loss.
                c = self.connect(profile=dropped)
                a, b = (c.create_session(str(uuid4())) for _ in range(2))
                ea, eb = a.edit("A"), b.edit("B")
                ea.create_parameter(str(uuid4()), "A", -1, 1, 0)
                eb.create_parameter(str(uuid4()), "B", -1, 1, 0)
                self.assert_code("OUTCOME_UNKNOWN", ea.commit)
                pending = ea.pending_request
                with self.assertRaises(zipzi.Busy):
                    with eb:
                        eb.commit()
                self.assertEqual(eb.state, "editing")
                self.assertIsNone(eb.pending_request)
                with self.assertRaises(RuntimeError):
                    eb.recover()
                self.assertEqual(ea.pending_request, pending)
                ea.recover()
                self.assertEqual(ea.state, "committed")
                self.assertEqual(a.history_lengths(), (1, 0))
                self.assertEqual(b.history_lengths(), (0, 0))
                eb.commit()
                self.assertEqual(b.history_lengths(), (1, 0))
                self.assertEqual(c.statistics()["edits"], 0)

    def test_blocked_apply_and_cancel_do_not_claim_another_edits_request(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "apply"
        c = self.connect()
        a, b = (c.create_session(str(uuid4())) for _ in range(2))
        ea, eb = a.edit("A"), b.edit("B")
        self.assert_code("OUTCOME_UNKNOWN", lambda: ea.create_parameter(str(uuid4()), "A", -1, 1, 0))
        pending = ea.pending_request
        self.assert_code("BUSY", lambda: eb.create_parameter(str(uuid4()), "B", -1, 1, 0))
        self.assert_code("BUSY", eb.cancel)
        self.assertEqual(eb.state, "editing")
        self.assertIsNone(eb.pending_request)
        self.assertEqual(ea.pending_request, pending)
        ea.recover()
        ea.commit()
        eb.cancel()
        self.assertEqual(a.history_lengths(), (1, 0))
        self.assertEqual(b.history_lengths(), (0, 0))

    def test_parameter_replacement_is_one_recoverable_apply(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "apply"
        os.environ["ZIPZI_TEST_DROP_RESPONSE_AFTER"] = "1"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        parameter = str(uuid4())
        with a.edit("seed") as e:
            e.create_parameter(parameter, "original", -1, 1, 0, runtime_id="ParamOriginal")
        with self.assertRaises(zipzi.OutcomeUnknown):
            with a.edit("replace") as e:
                e.replace_parameter(parameter, "replacement", -2, 2, .5)
        self.assertEqual(e.pending_request["operation"]["op"], "apply")
        e.recover()
        self.assertEqual(e.parameter(parameter).name, "replacement")
        self.assertEqual(e.parameter(parameter).runtime_id, "ParamOriginal")
        e.commit()
        self.assertEqual(a.parameter(parameter).name, "replacement")
        self.assertEqual(a.history_lengths(), (2, 0))
        a.undo()
        self.assertEqual(a.parameter(parameter).name, "original")

    def test_status_timeout_closes_socket_and_preserves_pending_edit(self):
        os.environ["ZIPZI_TEST_DROP_RESPONSE"] = "apply"
        os.environ["ZIPZI_TEST_STATUS_DELAY_MS"] = "150"
        c = self.connect()
        a = c.create_session(str(uuid4()))
        e = a.edit("recover after status timeout")
        self.assert_code("OUTCOME_UNKNOWN", lambda: e.create_parameter(str(uuid4()), "A", -1, 1, 0))
        pending = e.pending_request
        c._connect()
        c._socket.settimeout(.02)
        with self.assertRaises(TimeoutError):
            c.request_status(pending["request_id"])
        self.assertIsNone(c._socket)
        self.assertEqual(e.pending_request, pending)
        e.recover()
        e.commit()
        self.assertEqual(a.history_lengths(), (1, 0))
        self.assertEqual(c.list_sessions()[0]["id"], a.session_id)
        # Also discard a delayed status response when no edit is pending.
        c._socket.settimeout(.02)
        with self.assertRaises(TimeoutError):
            c.request_status(pending["request_id"])
        self.assertIsNone(c._socket)
        self.assertEqual(c.list_sessions()[0]["id"], a.session_id)

    def test_request_digest_reuse_expiry_and_restart(self):
        os.environ["ZIPZI_TEST_RESULT_COUNT"] = "2"
        c = self.connect()
        c.list_sessions()
        request = {"protocol": CONTRACT, "runtime": c.runtime_instance_id, "client": c.client_id,
            "request_id": c._next, "operation": {"op": "list_sessions"}}
        same = _exchange(c._socket, request)
        self.assertIn("ok", same)
        request["operation"] = {"op": "create_session"}
        self.assertEqual(_exchange(c._socket, request)["error"]["code"], "REQUEST_ID_REUSED")
        c.list_sessions(); c.list_sessions(); c.list_sessions()
        request["operation"] = {"op": "list_sessions"}
        self.assertEqual(_exchange(c._socket, request)["error"]["code"], "REQUEST_EXPIRED")
        old = c.runtime_instance_id
        os.kill(c.host_pid, signal.SIGTERM)
        try:
            os.waitpid(c.host_pid, 0)
        except ChildProcessError:
            pass
        fresh = self.connect()
        self.assertNotEqual(old, fresh.runtime_instance_id)
        self.assert_code("HOST_RESTARTED", c._connect)

    def test_protocol_mismatch_cannot_start_replacement(self):
        c = self.connect()
        stream = socket.socket(socket.AF_UNIX)
        try:
            stream.connect(str(c.endpoint))
            self.assertEqual(_exchange(stream, {"protocol": "wrong"})["error"]["code"], "PROTOCOL_MISMATCH")
        finally:
            stream.close()
        self.assert_code("BUILD_MISMATCH", lambda: self.connect(sdk_build="wrong"))
        self.assertEqual(len((self.directory / "pids").read_text().splitlines()), 1)

    def test_grace_releases_candidates_after_normal_exit_and_kill(self):
        os.environ["ZIPZI_TEST_GRACE_MS"] = "150"
        c = self.connect()
        session = c.create_session(str(uuid4()))
        sid = session._native.session_id
        program = '''import json,sys,time,zipzi
c=zipzi.connect(start=False)
s=c.attach_session(json.loads(sys.argv[1]))
e=s.edit("temporary")
e.create_parameter(sys.argv[2], "temporary", -1,1,0)
print("ready",flush=True)
if sys.argv[3]=="wait": time.sleep(60)
'''
        for mode in ("exit", "wait"):
            parameter = str(uuid4())
            child = subprocess.Popen([sys.executable, "-I", "-c", program, json.dumps(sid), parameter, mode], stdout=subprocess.PIPE, text=True)
            self.children.append(child)
            self.assertEqual(child.stdout.readline().strip(), "ready")
            if mode == "wait":
                child.kill()
            child.wait(timeout=5)
            child.stdout.close()
            time.sleep(.25)
            self.assertIsNone(session.parameter(parameter))
            self.assertEqual(session.history_lengths(), (0,0))
        # More than candidate/client budgets would permit without expiry.
        for _ in range(100):
            with session.edit("cancel") as edit:
                edit.cancel()
        self.assertEqual(session.history_lengths(), (0,0))
        self.assertEqual(c.host_pid, self.connect().host_pid)

    def test_hundred_connections_expire_without_edits_jobs_or_result_growth(self):
        os.environ["ZIPZI_TEST_GRACE_MS"] = "30"
        c = self.connect()
        session = c.create_session(str(uuid4()))
        baseline = c.statistics()["accounted_document_bytes"]
        rss_before = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(c.host_pid)], text=True).strip())
        for _ in range(100):
            other = zipzi.connect(start=False)
            bound = other.attach_session(session.session_id)
            edit = bound.edit("disconnect candidate")
            other.close()
            time.sleep(.045)
        statistics = c.statistics()
        self.assertEqual(statistics["edits"], 0)
        self.assertEqual(statistics["jobs"], 0)
        self.assertEqual(statistics["clients"], 2)  # Editor fixture + this client.
        self.assertEqual(statistics["accounted_document_bytes"], baseline)
        self.assertLess(statistics["request_results"], 8)
        rss_after = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(c.host_pid)], text=True).strip())
        print("R3_STRESS " + json.dumps(dict(statistics, iterations=100, host_rss_before_kib=rss_before, host_rss_after_kib=rss_after)), flush=True)

    def test_two_real_clients_cold_start_once_and_bind_unsaved_session(self):
        os.environ["ZIPZI_TEST_START_DELAY_MS"] = "150"
        script = '''import json,sys,time,zipzi
from pathlib import Path
c=zipzi.connect(host=sys.argv[1])
p=Path(sys.argv[2])
if sys.argv[3]=="create":
 s=c.create_session(sys.argv[4]); p.write_text(json.dumps(s._native.session_id))
else:
 while not p.exists(): time.sleep(.01)
 s=c.attach_session(json.loads(p.read_text()))
print(json.dumps([c.runtime_instance_id,c.host_pid,s._native.session_id]),flush=True)
'''
        children = [subprocess.Popen([sys.executable, "-I", "-c", script, str(HOST), str(self.directory / "session"), role, str(uuid4())], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for role in ("create", "attach")]
        self.children.extend(children)
        results = []
        for child in children:
            out, err = child.communicate(timeout=10)
            self.assertEqual(child.returncode, 0, err)
            results.append(json.loads(out))
        self.assertEqual(results[0], results[1])
        self.assertEqual(len((self.directory / "pids").read_text().splitlines()), 1)
        c = self.connect()
        self.assertEqual(c.list_sessions()[0]["id"], results[0][2])

    def test_slow_start_timeout_stale_socket_start_failure_retry(self):
        os.environ["ZIPZI_TEST_START_DELAY_MS"] = "250"
        self.assert_code("HOST_NOT_READY", lambda: self.connect(timeout=.03))
        c = self.connect(timeout=3)
        self.assertEqual(len((self.directory / "pids").read_text().splitlines()), 1)
        os.kill(c.host_pid, signal.SIGTERM)
        try:
            os.waitpid(c.host_pid, 0)
        except ChildProcessError:
            pass
        # SIGTERM leaves a stale socket. The next exclusive owner may clean it.
        self.assertTrue(c.endpoint.exists())
        os.environ["ZIPZI_TEST_START_DELAY_MS"] = "0"
        os.environ["ZIPZI_TEST_FAIL_START"] = "1"
        self.assert_code("HOST_START_FAILED", lambda: self.connect(timeout=2))
        del os.environ["ZIPZI_TEST_FAIL_START"]
        fresh = self.connect()
        self.assertNotEqual(c.runtime_instance_id, fresh.runtime_instance_id)


if __name__ == "__main__":
    unittest.main()
