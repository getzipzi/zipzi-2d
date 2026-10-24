"""One handwritten recipe on Local and Connected; independent geometry/history assertions."""
from dataclasses import FrozenInstanceError
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from uuid import UUID

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "crates/zipzi-python2/client/python"
sys.path[:0] = [str(SOURCE), os.environ["ZIPZI_PYTHON2_NATIVE"]]
import zipzi
from zipzi import meshes, ops, resources

def uid(n: int) -> str:
    return f"79000000-0000-4000-8000-{n:012x}"

GEOMETRY = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
    ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))

class PythonSdk(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zzpy1-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name)
        self.counter = 0

    def runtime(self, mode, *, drop=None, drop_after=0):
        if mode == "local":
            runtime = zipzi.local()
        else:
            self.counter += 1
            directory = self.folder / str(self.counter)
            directory.mkdir(mode=0o700)
            env = dict(os.environ, ZIPZI_RUNTIME_DIR=str(directory), ZIPZI_RUNTIME_PROFILE="python2")
            if drop:
                env["ZIPZI_TEST_DROP_RESPONSE"] = drop
                env["ZIPZI_TEST_DROP_RESPONSE_AFTER"] = str(drop_after)
            host = subprocess.Popen([os.environ["ZIPZI_PYTHON2_HOST"]], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            def stop():
                host.terminate()
                host.wait(timeout=5)
                if host.stderr:
                    host.stderr.close()
            self.addCleanup(stop)
            self.stop_host = stop
            endpoint = directory / "python2.sock"
            deadline = time.monotonic() + 10
            while True:
                if host.poll() is not None:
                    self.fail(host.stderr.read().decode() if host.stderr else "Host failed")
                try:
                    runtime = zipzi.connect(endpoint=endpoint)
                    break
                except (FileNotFoundError, ConnectionRefusedError):
                    if time.monotonic() >= deadline:
                        self.fail("Host startup timed out")
                    time.sleep(.01)
        self.addCleanup(runtime.close)
        return runtime

    def recipe(self, runtime, destination):
        session = runtime.sessions.create(uid(1))
        self.assertEqual(session.info().history, (0, 0))
        with session.edit("face") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            self.assertEqual(UUID(face).version, 4)
            snapshot = edit.meshes.get(face)
            self.assertEqual(snapshot.source.sequence, 1)
            self.assertIsNotNone(snapshot.source.edit)
            self.assertEqual(snapshot.geometry.vertex_ids, (42, 7, 99))
            with self.assertRaises(zipzi.SdkFailure) as error:
                session.meshes.get(face)
            self.assertEqual(error.exception.code, "MISSING_MESH")
            positions = ((2., 3.), (3., 3.), (2., 4.))
            result = edit.meshes.set_positions(face, vertex_ids=snapshot.geometry.vertex_ids, positions=positions)
            self.assertEqual(result.source.sequence, 2)
            self.assertEqual(result.source.edit, snapshot.source.edit)
            self.assertEqual(edit.meshes.get(face).geometry.positions, positions)
            self.assertEqual(session.info().history, (0, 0))
            with self.assertRaises(FrozenInstanceError):
                snapshot.name = "mutated"
        formal = session.meshes.get(face)
        self.assertEqual(formal.source.version, session.info().version)
        self.assertIsNone(formal.source.edit)
        self.assertEqual(formal.source.runtime, snapshot.source.runtime)
        self.assertEqual(formal.source.session, snapshot.source.session)
        self.assertEqual(session.info().history, (1, 0))
        with session.edit("batch") as edit:
            result = edit.apply_many((ops.Create(meshes.MeshSpec("New", GEOMETRY, id=uid(2))),
                ops.Rename(uid(2), "Batch"), ops.Rename(face, "Final")))
            self.assertEqual(result.ids, (uid(2), uid(2), face))
            self.assertEqual(result.source.sequence, 1)
            self.assertEqual(edit.meshes.get(uid(2)).name, "Batch")
        self.assertEqual(session.info().history, (2, 0))
        self.assertTrue(session.undo().wait().changed)
        self.assertEqual(session.meshes.get(face).name, "Face")
        self.assertEqual(session.info().history, (1, 1))
        with session.edit("no op"):
            pass
        with session.edit("cancel") as edit:
            edit.meshes.rename(face, "Discard")
            edit.cancel()
        self.assertEqual(session.info().history, (1, 1))
        session.redo().wait()
        saved = session.project.save(destination).wait()
        self.assertTrue(saved.manifest.is_file())
        self.assertFalse(session.info().modified)
        return face, saved.manifest, (session.meshes.get(face).name, formal.geometry.positions, session.info().history)

    def test_shared_recipe(self):
        outputs = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                face, manifest, result = self.recipe(runtime, self.folder / mode)
                runtime.close()
                if mode == "connected":
                    # Disconnect retains sessions; stop this fixture explicitly
                    # to release its project lock before a fresh disk open.
                    self.stop_host()
                reopened = self.runtime(mode).sessions.open(manifest).wait()
                self.assertEqual(reopened.meshes.get(face).name, "Final")
                self.assertEqual(reopened.meshes.get(face).geometry.positions, ((2., 3.), (3., 3.), (2., 4.)))
                self.assertEqual(reopened.info().history, (0, 0))
                self.assertFalse(reopened.info().modified)
                outputs.append(result)
        self.assertEqual(outputs[0], outputs[1])

    def test_caught_domain_and_conversion_errors_poison_the_entire_candidate(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            for failure in ("domain", "encoding", "nan"):
                with self.subTest(mode=mode, failure=failure):
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        with session.edit("fail") as edit:
                            edit.meshes.create(meshes.MeshSpec("Draft", GEOMETRY, id=uid(2)))
                            try:
                                if failure == "domain":
                                    edit.apply_many((ops.Rename(uid(2), "Partial"), ops.Rename(uid(999), "Missing")))
                                else:
                                    edit.meshes.set_positions(uid(2), vertex_ids=(2**40 if failure == "encoding" else 42,),
                                        positions=((float("nan") if failure == "nan" else 1., 0.),))
                            except (zipzi.SdkFailure, ValueError):
                                pass
                    self.assertEqual(error.exception.code, "EDIT_ABORTED")
                    self.assertEqual(session.info().history, (0, 0))
                    with self.assertRaises(zipzi.SdkFailure):
                        session.meshes.get(uid(2))

    def test_admission_rejection_retains_candidate_and_source(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                session = self.runtime(mode).sessions.create(uid(1))
                with session.edit("admission") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("A", GEOMETRY))
                    before = edit.meshes.get(face).source
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        edit.apply_many((ops.Rename(face, "x"),) * 1025)
                    self.assertEqual(error.exception.code, "BUDGET_EXCEEDED")
                    self.assertEqual(edit.meshes.get(face).source, before)
                    edit.meshes.rename(face, "B")
                self.assertEqual(session.meshes.get(face).name, "B")

    def test_connected_apply_and_commit_loss_recover_same_request_once(self):
        for lost in ("apply_authoring", "commit", "job"):
            with self.subTest(lost=lost):
                session = self.runtime("connected", drop=lost).sessions.create(uid(1))
                edit = session.edit("recover")
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    with edit:
                        edit.apply(ops.Create(meshes.MeshSpec("Once", GEOMETRY, id=uid(2))))
                if lost != "job":
                    pending = edit.pending_request
                    self.assertIsNotNone(pending)
                    self.assertEqual(error.exception.pending, pending)
                result = edit.recover()
                self.assertIsNone(edit.pending_request)
                if lost == "apply_authoring":
                    self.assertEqual(result.source.sequence, 1)
                    edit.commit().wait()
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.meshes.get(uid(2)).name, "Once")

    def test_prepared_images_plans_mixed_batch_history_save_and_reopen(self):
        outputs = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                a = runtime.resources.prepare_image(1, 1, bytearray((3, 5, 7, 255)))
                b = runtime.resources.prepare_image(1, 1, memoryview(bytes((11, 13, 17, 255))))
                self.assertEqual((a.width, a.height), (1, 1))
                self.assertEqual(runtime.resources.usage().rgba_bytes, 8)
                with session.edit("texture and mesh") as edit:
                    result = edit.apply_many((ops.CreateImage("A", a, id=uid(2)),
                        ops.Create(meshes.MeshSpec("Face", GEOMETRY, texture_asset_id=uid(2), id=uid(3)))))
                    self.assertEqual(result.ids, (uid(2), uid(3)))
                    self.assertEqual(result.source.sequence, 1)
                    snapshot = edit.resources.get(uid(2))
                    self.assertEqual(snapshot.pixels.rgba, bytes((3, 5, 7, 255)))
                    a.close()
                    a.close()
                    plan = edit.resources.plan(resources.RenameIntent(uid(2), "A", "Texture"))
                    report = plan.report
                    self.assertEqual(report.source, snapshot.source)
                    self.assertEqual(report.affected_ids, (uid(2),))
                    self.assertFalse(report.no_op)
                    with self.assertRaises(FrozenInstanceError):
                        report.no_op = True
                    result = edit.apply_plan(plan)
                    self.assertEqual(result.source.sequence, 2)
                    plan.close()
                    self.assertIs(plan.report, report)
                    self.assertEqual(edit.resources.get(uid(2)).name, "Texture")
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(b.pixels().rgba, bytes((11, 13, 17, 255)))
                b.close()
                self.assertEqual(runtime.resources.usage().rgba_bytes, 4)
                with session.meshes.plan(meshes.RenameIntent(uid(3), "Face", "Final")) as plan:
                    self.assertEqual(plan.report.source, session.meshes.get(uid(3)).source)
                    with session.edit("formal plan and trailing operation") as edit:
                        result = edit.apply_many((ops.ApplyPlan(plan), ops.Rename(uid(3), "Final batch")))
                        self.assertEqual(result.source.sequence, 1)
                session.undo().wait()
                self.assertEqual(session.meshes.get(uid(3)).name, "Face")
                self.assertEqual(session.resources.get(uid(2)).pixels.rgba, snapshot.pixels.rgba)
                session.redo().wait()
                self.assertEqual(session.info().history, (2, 0))
                saved = session.project.save(self.folder / mode).wait()
                outputs.append((session.meshes.get(uid(3)).name, session.resources.get(uid(2)).pixels))
                runtime.close()
                if mode == "connected":
                    self.stop_host()
                reopened = self.runtime(mode).sessions.open(saved.manifest).wait()
                self.assertEqual(reopened.meshes.get(uid(3)).name, "Final batch")
                self.assertEqual(reopened.resources.get(uid(2)).pixels.rgba, snapshot.pixels.rgba)
                self.assertEqual(reopened.info().history, (0, 0))
        self.assertEqual(outputs[0], outputs[1])

    def test_replacing_image_retains_old_snapshots_and_exact_undo_redo_pixels(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                a, b = (runtime.resources.prepare_image(1, 1, pixels) for pixels in (b"abcd", b"efgh"))
                with session.edit("A") as edit:
                    asset = edit.resources.create_image("Texture", a)
                    self.assertEqual(UUID(asset).version, 4)
                old = session.resources.get(asset)
                a.close()
                with session.edit("B") as edit:
                    edit.resources.replace_image(asset, b)
                    b.close()
                    self.assertEqual(edit.resources.get(asset).pixels.rgba, b"efgh")
                    self.assertEqual(session.resources.get(asset).pixels.rgba, b"abcd")
                self.assertEqual(session.resources.get(asset).name, "Texture")
                self.assertEqual(session.resources.get(asset).pixels.rgba, b"efgh")
                session.undo().wait()
                self.assertEqual(session.resources.get(asset).pixels.rgba, b"abcd")
                session.redo().wait()
                self.assertEqual(session.resources.get(asset).pixels.rgba, b"efgh")
                self.assertEqual(old.pixels.rgba, b"abcd")
                self.assertEqual(session.info().history, (2, 0))

    def test_stale_plans_keep_reports_and_abort_without_partial_publication(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            with session.edit("base") as edit:
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            for source in ("formal", "candidate", "other", "batch"):
                with self.subTest(mode=mode, source=source):
                    plan = session.meshes.plan(meshes.RenameIntent(face, "Face", "Planned")) if source in {"formal", "batch"} else None
                    edit = session.edit("stale")
                    if source in {"candidate", "other"}:
                        plan = edit.meshes.plan(meshes.RenameIntent(face, "Face", "Planned"))
                    if source == "other":
                        edit.cancel()
                        edit = session.edit("another candidate")
                    elif source != "batch":
                        edit.meshes.rename(face, "Draft")
                    report = plan.report
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        edit.apply_many((ops.Rename(face, "Partial"), ops.ApplyPlan(plan))) if source == "batch" else edit.apply_plan(plan)
                    self.assertEqual(error.exception.code, "STALE_PLAN")
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.commit()
                    self.assertIs(plan.report, report)
                    plan.close()
                    self.assertIs(plan.report, report)
                    self.assertEqual(session.meshes.get(face).name, "Face")
                    self.assertEqual(session.info().history, (1, 0))
                    edit.cancel()

    def test_foreign_and_closed_handles_poison_the_candidate(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            other = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            image = other.resources.prepare_image(1, 1, bytes((1, 2, 3, 255)))
            for state in ("foreign", "closed"):
                with self.subTest(mode=mode, state=state):
                    if state == "closed":
                        image = runtime.resources.prepare_image(1, 1, bytes((4, 5, 6, 255)))
                        image.close()
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        with session.edit("invalid lease") as edit:
                            edit.resources.create_image("Invalid", image)
                    self.assertEqual(error.exception.code, "WRONG_OWNER" if state == "foreign" else "INVALID_REFERENCE")
                    self.assertEqual(session.info().history, (0, 0))
                    image.close()

    def test_independent_preparation_and_hundred_cancelled_leases_return_to_zero(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                for _ in range(100):
                    edit = session.edit("cancel")
                    with self.assertRaises(zipzi.SdkFailure):
                        runtime.resources.prepare_image(1, 1, b"bad")
                    image = runtime.resources.prepare_image(1, 1, b"rgba")
                    asset = edit.resources.create_image("Draft", image)
                    plan = edit.resources.plan(resources.RenameIntent(asset, "Draft", "Planned"))
                    pixels = edit.resources.get(asset).pixels
                    image.close()
                    edit.cancel()
                    # A candidate Plan owns its source snapshot until release.
                    self.assertEqual(runtime.resources.usage().rgba_bytes, 4)
                    plan.close()
                    usage = runtime.resources.usage()
                    self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes, usage.preparing_bytes), (0, 0, 0, 0))
                    self.assertEqual(pixels.rgba, b"rgba")
                self.assertEqual(session.info().history, (0, 0))

    def test_resource_and_plan_response_losses_keep_exact_recovery_owner(self):
        for lost in ("prepare_image", "prepared_pixels", "image_snapshot", "plan_authoring", "release_prepared_image", "release_modeling_plan", "statistics"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                if lost == "prepare_image":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.resources.prepare_image(1, 1, b"rgba")
                    image = error.exception.recover()
                    self.assertIsInstance(image, zipzi.PreparedImage)
                else:
                    image = runtime.resources.prepare_image(1, 1, b"rgba")
                if lost == "prepared_pixels":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        image.pixels()
                    with self.assertRaises(zipzi.Busy):
                        image.close()
                    self.assertEqual(error.exception.recover().rgba, b"rgba")
                    with self.assertRaises(zipzi.SdkFailure) as resolved:
                        error.exception.recover()
                    self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")
                edit = session.edit("recover handles")
                asset = edit.resources.create_image("Draft", image)
                if lost == "image_snapshot":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        edit.resources.get(asset)
                    self.assertEqual(error.exception.recover().pixels.rgba, b"rgba")
                if lost == "plan_authoring":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        edit.resources.plan(resources.RenameIntent(asset, "Draft", "Final"))
                    plan = error.exception.recover()
                    self.assertIsInstance(plan, zipzi.Plan)
                else:
                    plan = edit.resources.plan(resources.RenameIntent(asset, "Draft", "Final"))
                report = plan.report
                with patch.object(runtime._backend, "plan", side_effect=AssertionError("Report made an RPC")):
                    self.assertIs(plan.report, report)
                edit.apply_plan(plan)
                edit.commit().wait()
                for handle, target in ((image, "release_prepared_image"), (plan, "release_modeling_plan")):
                    if lost == target:
                        with self.assertRaises(zipzi.OutcomeUnknown) as error:
                            handle.close()
                        self.assertIsNone(error.exception.recover())
                        handle.close()
                        with self.assertRaises(zipzi.SdkFailure):
                            handle._check(runtime._backend)
                    else:
                        handle.close()
                if lost == "statistics":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.resources.usage()
                    usage = error.exception.recover()
                else:
                    usage = runtime.resources.usage()
                self.assertEqual((usage.prepared_images, usage.plans), (0, 0))
                self.assertEqual(session.resources.get(asset).name, "Final")
                self.assertEqual(session.info().history, (1, 0))

    def test_plan_apply_loss_and_foreign_closed_plan_references(self):
        runtime = self.runtime("connected", drop="apply_authoring", drop_after=1)
        session = runtime.sessions.create(uid(1))
        with session.edit("base") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        with session.meshes.plan(meshes.RenameIntent(face, "Face", "Final")) as plan:
            edit = session.edit("lost plan apply")
            with self.assertRaises(zipzi.OutcomeUnknown) as error:
                edit.apply_plan(plan)
            result = error.exception.recover()
            self.assertEqual(result.source.sequence, 1)
            edit.commit().wait()
        self.assertEqual(session.meshes.get(face).name, "Final")
        self.assertEqual(session.info().history, (2, 0))
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime, other = self.runtime(mode), self.runtime(mode)
                session, other_session = runtime.sessions.create(uid(1)), other.sessions.create(uid(2))
                with other_session.edit("other") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Other", GEOMETRY))
                plan = other_session.meshes.plan(meshes.RenameIntent(face, "Other", "Planned"))
                with self.assertRaises(zipzi.SdkFailure) as error:
                    with session.edit("foreign") as edit:
                        edit.apply_plan(plan)
                self.assertEqual(error.exception.code, "WRONG_OWNER")
                plan.close()
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                plan = session.meshes.plan(meshes.RenameIntent(face, "Face", "Planned"))
                report = plan.report
                plan.close()
                with self.assertRaises(zipzi.SdkFailure) as error:
                    with session.edit("closed") as edit:
                        edit.apply_plan(plan)
                self.assertEqual(error.exception.code, "INVALID_REFERENCE")
                self.assertIs(plan.report, report)
                self.assertEqual(session.meshes.get(face).name, "Face")
                self.assertEqual(session.info().history, (1, 0))

    def test_failed_read_or_plan_does_not_poison_candidate(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                session = self.runtime(mode).sessions.create(uid(1))
                with session.edit("read failures") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    source = edit.meshes.get(face).source
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.resources.get(uid(999))
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.meshes.plan(meshes.RenameIntent(face, "Wrong expectation", "Planned"))
                    self.assertEqual(edit.meshes.get(face).source, source)
                    edit.meshes.rename(face, "Final")
                self.assertEqual(session.meshes.get(face).name, "Final")
                self.assertEqual(session.info().history, (1, 0))

    def test_mixed_resource_batch_response_loss_does_not_repeat_a_write(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        image = runtime.resources.prepare_image(1, 1, b"rgba")
        edit = session.edit("mixed lost reply")
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.apply_many((ops.CreateImage("Draft", image, uid(2)), ops.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(3)))))
        result = error.exception.recover()
        self.assertEqual(result.source.sequence, 1)
        self.assertEqual(result.ids, (uid(2), uid(3)))
        edit.commit().wait()
        image.close()
        self.assertEqual(session.info().history, (1, 0))
        self.assertEqual(session.resources.get(uid(2)).pixels.rgba, b"rgba")

    def test_local_wait_releases_gil_and_close_is_idempotent(self):
        runtime = self.runtime("local")
        session = runtime.sessions.create(uid(1))
        edit = session.edit("background")
        edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        job = edit.commit()
        result = []
        thread = threading.Thread(target=lambda: result.append(job.wait()))
        thread.start()
        thread.join(timeout=5)
        self.assertFalse(thread.is_alive(), "Native wait held the GIL or failed to consume completion")
        self.assertTrue(result[0].changed)
        self.assertEqual(job.wait(), result[0])
        runtime.close()
        runtime.close()
        with self.assertRaises(zipzi.SdkFailure) as error:
            session.info()
        self.assertEqual(error.exception.code, "HOST_CLOSED")

    def test_connected_lifecycle_and_read_response_loss_keep_a_recovery_owner(self):
        for lost in ("create_session", "begin_edit", "mesh_snapshot", "session_info", "save", "undo", "redo"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                if lost == "create_session":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.sessions.create(uid(1))
                    session = error.exception.recover()
                    self.assertIsInstance(session, zipzi.Session)
                else:
                    session = runtime.sessions.create(uid(1))
                if lost == "begin_edit":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.edit("begin")
                    edit = error.exception.recover()
                else:
                    edit = session.edit("begin")
                with edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    if lost == "mesh_snapshot":
                        with self.assertRaises(zipzi.OutcomeUnknown) as error:
                            edit.meshes.get(face)
                        snapshot = error.exception.recover()
                        self.assertEqual(snapshot.source.sequence, 1)
                        self.assertEqual(snapshot.name, "Face")
                        edit.meshes.rename(face, "After query")
                if lost == "session_info":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.info()
                    self.assertEqual(error.exception.recover().history, (1, 0))
                if lost == "save":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.project.save(self.folder / "save-loss")
                    job = error.exception.recover()
                    self.assertTrue(job.wait().manifest.is_file())
                if lost in {"undo", "redo"}:
                    if lost == "redo":
                        session.undo().wait()
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.undo() if lost == "undo" else session.redo()
                    job = error.exception.recover()
                    self.assertTrue(job.wait().changed)
                    self.assertEqual(session.info().history, (0, 1) if lost == "undo" else (1, 0))

    def test_connected_cleanup_response_loss_never_covers_the_original_error(self):
        session = self.runtime("connected", drop="release_edit").sessions.create(uid(1))
        edit = session.edit("bad conversion")
        with self.assertRaises(ValueError) as error:
            with edit:
                edit.meshes.create(meshes.MeshSpec("Partial", GEOMETRY, id=uid(2)))
                edit.meshes.set_positions(uid(2), vertex_ids=(2**40,), positions=((0., 0.),))
        self.assertTrue(error.exception.__notes__)
        self.assertIsNotNone(edit.pending_request)
        self.assertIsNone(edit.recover())
        self.assertEqual(session.info().history, (0, 0))
        with self.assertRaises(zipzi.SdkFailure):
            session.meshes.get(uid(2))

    def test_another_edit_cannot_recover_or_poison_the_pending_owner(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        other = runtime.sessions.create(uid(3))
        edit, other_edit = session.edit("owner"), other.edit("other")
        with self.assertRaises(zipzi.OutcomeUnknown):
            edit.apply(ops.Create(meshes.MeshSpec("Once", GEOMETRY, id=uid(2))))
        with self.assertRaises(zipzi.Busy):
            other_edit.apply(ops.Create(meshes.MeshSpec("Other", GEOMETRY, id=uid(4))))
        self.assertIsNone(other_edit.pending_request)
        with self.assertRaises(zipzi.SdkFailure) as error:
            other_edit.recover()
        self.assertEqual(error.exception.code, "NO_PENDING_REQUEST")
        edit.recover()
        edit.commit().wait()
        other_edit.apply(ops.Create(meshes.MeshSpec("Other", GEOMETRY, id=uid(4))))
        other_edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))
        self.assertEqual(other.info().history, (1, 0))

    def test_values_import_without_extension_or_host(self):
        code = f"""import sys
sys.path.insert(0, {str(SOURCE)!r})
import zipzi
from zipzi import meshes, ops, resources
assert 'zipzi_native._native' not in sys.modules
assert zipzi.SdkFailure.__module__ == 'zipzi._errors'
assert meshes.Geometry((0,), ((0., 0.),), ((0., 0.),), ()).vertex_ids == (0,)
"""
        subprocess.run([sys.executable, "-I", "-c", code], check=True, cwd="/tmp")

    def test_local_bridge_does_not_encode_or_decode_python_json(self):
        runtime = self.runtime("local")
        with patch("json.dumps", side_effect=AssertionError("Local bridge encoded JSON")), \
             patch("json.loads", side_effect=AssertionError("Local bridge decoded JSON")):
            session = runtime.sessions.create(uid(1))
            with session.edit("direct") as edit:
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                edit.apply(ops.Rename(face, "Direct"))
                self.assertEqual(edit.meshes.get(face).name, "Direct")
                with runtime.resources.prepare_image(1, 1, b"rgba") as image:
                    asset = edit.resources.create_image("Draft", image)
                    self.assertEqual(image.pixels().rgba, b"rgba")
                    with edit.resources.plan(resources.RenameIntent(asset, "Draft", "Image")) as plan:
                        edit.apply_plan(plan)
                        self.assertEqual(plan.report.source.sequence, 3)
                    self.assertEqual(edit.resources.get(asset).pixels.rgba, b"rgba")
            with session.project.save(self.folder / "direct-save") as job:
                saved = job.wait()
            self.assertEqual(job.wait(), saved)
            self.assertTrue(saved.manifest.is_file())

if __name__ == "__main__":
    unittest.main()
