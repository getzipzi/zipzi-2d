"""One handwritten recipe on Local and Connected; independent geometry/history assertions."""
from dataclasses import FrozenInstanceError
import gc
import weakref
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

    def runtime(self, mode, *, drop=None, drop_after=0, settings=None, directory=None):
        if mode == "local":
            runtime = zipzi.local()
        else:
            self.counter += 1
            directory = directory or self.folder / str(self.counter)
            directory.mkdir(mode=0o700, exist_ok=True)
            env = dict(os.environ, ZIPZI_RUNTIME_DIR=str(directory), ZIPZI_RUNTIME_PROFILE="python2")
            env.update(settings or {})
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

    def test_session_listing_identity_and_attach_use_the_same_runtime(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(711))
                info, = runtime.sessions.list()
                self.assertEqual(info.id, session.id)
                self.assertEqual(info.state, "ready")
                self.assertIsNone(info.path)
                attached = runtime.sessions.attach(info.id)
                with attached.edit("same session") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Resident", GEOMETRY))
                self.assertEqual(session.meshes.get(face).name, "Resident")
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.info().id, info.id)
                self.assertEqual(session.meshes.get(face).source.runtime, info.id.runtime)
                with zipzi.local() as foreign:
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        foreign.sessions.attach(info.id)
                    self.assertEqual(error.exception.code, "INVALID_REFERENCE")
                if mode == "local":
                    self.assertIsNone(runtime.connection)
                else:
                    self.assertEqual(runtime.connection.runtime, info.id.runtime)

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

    def control(self, runtime, action):
        return runtime._backend._test_control(action)

    def await_io(self, runtime):
        deadline = time.monotonic() + 5
        while not self.control(runtime, "entered"):
            self.assertLess(time.monotonic(), deadline, "Worker did not enter IO")
            time.sleep(.005)

    def test_queued_cancel_releases_candidate_and_retains_terminal_error(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                image = runtime.resources.prepare_image(1, 1, b"rgba")
                edit = session.edit("queued")
                asset = edit.resources.create_image("Draft", image)
                image.close()
                self.control(runtime, "pause_dispatch")
                job = edit.commit()
                self.assertEqual(job.state(), "queued")
                job.cancel()
                self.assertEqual(job.state(), "cancelling")
                self.control(runtime, "resume_dispatch")
                with self.assertRaises(zipzi.SdkFailure) as error:
                    job.wait()
                self.assertEqual(error.exception.code, "CANCELLED")
                with self.assertRaises(zipzi.SdkFailure) as again:
                    job.close()
                self.assertIs(again.exception, error.exception)
                self.assertEqual(job.state(), "cancelled")
                self.assertEqual(session.info().history, (0, 0))
                with self.assertRaises(zipzi.SdkFailure):
                    session.resources.get(asset)
                self.assertEqual(runtime.resources.usage().rgba_bytes, 0)

    def test_cancellation_after_entering_io_reports_actual_save(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                self.control(runtime, "hold_io")
                job = session.project.save(self.folder / mode)
                self.await_io(runtime)
                job.cancel()
                self.assertEqual(job.state(), "cancelling")
                self.control(runtime, "release_io")
                saved = job.wait()
                self.assertTrue(saved.manifest.is_file())
                self.assertEqual(job.state(), "succeeded")
                self.assertIs(job.wait(), saved)
                self.assertEqual(session.info().history, (1, 0))
                self.assertFalse(session.info().modified)
                self.assertEqual(session.meshes.get(face).name, "Face")

    def test_worker_failure_and_panic_have_distinct_recovery_scopes(self):
        for mode in ("local", "connected"):
            for fault, expected in (("fail_next", "EXECUTOR_FAILURE"), ("panic_next", "WORKER_PANIC")):
                with self.subTest(mode=mode, fault=fault):
                    runtime = self.runtime(mode)
                    session = runtime.sessions.create(uid(1))
                    with session.edit("base") as edit:
                        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    old = session.meshes.get(face)
                    self.control(runtime, fault)
                    job = session.project.save(self.folder / f"{mode}-{fault}")
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        job.wait()
                    self.assertEqual(error.exception.code, expected)
                    self.assertEqual(job.state(), "failed")
                    with self.assertRaises(zipzi.SdkFailure) as again:
                        job.wait()
                    self.assertIs(again.exception, error.exception)
                    self.assertEqual(session.meshes.get(face), old)
                    self.assertEqual(session.info().history, (1, 0))
                    if fault == "panic_next":
                        with self.assertRaises(zipzi.SdkFailure) as faulted:
                            session.edit("unsafe")
                        self.assertEqual(faulted.exception.code, "FAULTED")
                    else:
                        with session.edit("retry") as edit:
                            edit.meshes.rename(face, "After rejection")
                        self.assertEqual(session.info().history, (2, 0))

    def test_cancel_and_state_response_loss_cannot_be_consumed_by_wait(self):
        for lost in ("cancel_job", "job_status"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                edit = session.edit("pending")
                edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                self.control(runtime, "pause_dispatch")
                job = edit.commit()
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    job.cancel() if lost == "cancel_job" else job.state()
                with self.assertRaises(zipzi.Busy):
                    job.wait()
                self.assertEqual(error.exception.recover(), None if lost == "cancel_job" else "queued")
                self.control(runtime, "resume_dispatch")
                if lost == "cancel_job":
                    with self.assertRaises(zipzi.SdkFailure) as cancelled:
                        job.wait()
                    self.assertEqual(cancelled.exception.code, "CANCELLED")
                else:
                    self.assertTrue(job.wait().changed)

    def test_expired_request_never_replays_a_write_under_a_new_id(self):
        for lost in ("apply_authoring", "prepare_image", "plan_authoring", "commit", "job"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost, settings={"ZIPZI_TEST_RESULT_TTL_MS": "30"})
                session = runtime.sessions.create(uid(1))
                edit = session.edit("expiry")
                if lost in {"plan_authoring", "commit", "job"}:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    if lost == "apply_authoring":
                        edit.meshes.create(meshes.MeshSpec("Once", GEOMETRY))
                    elif lost == "prepare_image":
                        runtime.resources.prepare_image(1, 1, b"rgba")
                    elif lost == "plan_authoring":
                        edit.meshes.plan(meshes.RenameIntent(face, "Face", "Planned"))
                    elif lost == "commit":
                        edit.commit()
                    else:
                        pending_job = edit.commit()
                        pending_job.wait()
                request_id = runtime._backend._next
                time.sleep(.08)
                with self.assertRaises(zipzi.SdkFailure) as expired:
                    error.exception.recover()
                self.assertEqual(expired.exception.code, "REQUEST_EXPIRED")
                self.assertEqual(runtime._backend._next, request_id)
                if lost == "job":
                    with self.assertRaises(zipzi.SdkFailure) as unknown_state:
                        pending_job.state()
                    self.assertEqual(unknown_state.exception.code, "REQUEST_EXPIRED")
                self.assertEqual(session.info().history, (1, 0) if lost in {"commit", "job"} else (0, 0))

    def test_client_expiry_reclaims_prepared_plan_and_candidate_without_new_identity(self):
        runtime = self.runtime("connected", drop="plan_authoring", settings={"ZIPZI_TEST_GRACE_MS": "30"})
        session = runtime.sessions.create(uid(1))
        image = runtime.resources.prepare_image(1, 1, b"rgba")
        edit = session.edit("expired client")
        asset = edit.resources.create_image("Draft", image)
        snapshot = edit.resources.get(asset)
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.resources.plan(resources.RenameIntent(asset, "Draft", "Planned"))
        old_client, old_id = runtime._backend._client, runtime._backend._next
        time.sleep(.1)
        with self.assertRaises(zipzi.SdkFailure) as expired:
            error.exception.recover()
        self.assertEqual(expired.exception.code, "CLIENT_EXPIRED")
        self.assertEqual((runtime._backend._client, runtime._backend._next), (old_client, old_id))
        observer = zipzi.connect(endpoint=runtime._backend._endpoint)
        self.addCleanup(observer.close)
        usage = observer.resources.usage()
        self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes), (0, 0, 0))
        self.assertEqual(snapshot.pixels.rgba, b"rgba")
        observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
        self.assertEqual(zipzi.Session(observer, session._token).info().history, (0, 0))

    def test_host_restart_refuses_original_pending_request(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        edit = session.edit("old runtime")
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.meshes.create(meshes.MeshSpec("Once", GEOMETRY))
        endpoint, old_id = runtime._backend._endpoint, runtime._backend._next
        self.stop_host()
        with self.assertRaises(zipzi.OutcomeUnknown) as unavailable:
            error.exception.recover()
        self.assertEqual(unavailable.exception.pending, error.exception.pending)
        fresh = self.runtime("connected", directory=endpoint.parent)
        with self.assertRaises(zipzi.SdkFailure) as restarted:
            error.exception.recover()
        self.assertEqual(restarted.exception.code, "HOST_RESTARTED")
        self.assertEqual(runtime._backend._next, old_id)
        self.assertEqual(fresh.resources.usage().prepared_images, 0)

    def test_idle_completion_and_close_during_entered_io(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            edit = session.edit("idle")
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            job = edit.commit()
            time.sleep(.08)
            self.assertEqual(job.state(), "succeeded")
            self.assertEqual(session.info().history, (1, 0))
            self.assertEqual(session.meshes.get(face).name, "Face")
            job.wait()
        runtime = self.runtime("local")
        session = runtime.sessions.create(uid(2))
        self.control(runtime, "hold_io")
        session.project.save(self.folder / "close-inflight")
        self.await_io(runtime)
        thread = threading.Thread(target=runtime.close)
        thread.start()
        time.sleep(.02)
        self.assertTrue(thread.is_alive(), "close returned before entered IO finished")
        self.control(runtime, "release_io")
        thread.join(timeout=5)
        self.assertFalse(thread.is_alive())
        manifest = self.folder / "close-inflight/project.zipzi.json"
        self.assertTrue(manifest.is_file(), "Entered save was silently rolled back by close")
        reopened = self.runtime("local").sessions.open(manifest).wait()
        self.assertFalse(reopened.info().modified)

    def test_retained_job_budget_refusal_keeps_edit_retryable(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                jobs = [runtime.sessions.create(uid(100 + n)).project.save(self.folder / mode / str(n)) for n in range(8)]
                session = runtime.sessions.create(uid(999))
                edit = session.edit("quota")
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                source = edit.meshes.get(face).source
                with self.assertRaises(zipzi.SdkFailure) as error:
                    edit.commit()
                self.assertEqual(error.exception.code, "BUDGET_EXCEEDED")
                self.assertEqual(edit.meshes.get(face).source, source)
                jobs[0].wait()
                edit.commit().wait()
                self.assertEqual(session.info().history, (1, 0))
                for job in jobs[1:]:
                    job.wait()

    def test_real_save_failure_releases_result_and_path_without_publication(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                before = session.info()
                invalid = self.folder / f"{mode}-file"
                self.control(runtime, "hold_io")
                job = session.project.save(invalid)
                self.await_io(runtime)
                invalid.write_bytes(b"keep")
                self.control(runtime, "release_io")
                try:
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        job.wait()
                    with self.assertRaises(zipzi.SdkFailure) as again:
                        job.wait()
                    self.assertIs(again.exception, error.exception)
                except zipzi.SdkFailure as error:
                    self.fail(f"Expected admitted IO failure, got preflight: {error}")
                self.assertEqual(invalid.read_bytes(), b"keep")
                self.assertEqual(session.info(), before)
                self.assertEqual(session.meshes.get(face).name, "Face")
                self.assertTrue(session.project.save(self.folder / f"{mode}-retry").wait().manifest.is_file())

    def test_last_native_owner_releases_project_lock_and_unsaved_memory(self):
        runtime = zipzi.local()
        session = runtime.sessions.create(uid(1))
        with runtime.resources.prepare_image(1, 1, b"rgba") as image:
            with session.edit("memory") as edit:
                asset = edit.resources.create_image("Texture", image)
        saved = session.project.save(self.folder / "last-owner").wait()
        pixels = session.resources.get(asset).pixels
        unused = runtime.resources.prepare_image(1, 1, b"abcd")
        edit = session.edit("never publish or autosave")
        draft = edit.resources.create_image("Unsaved", unused)
        unused.close()
        # Saved-image caches may be evicted; the unsaved candidate must still
        # pin its own actual RGBA allocation until the host is dropped.
        self.assertGreaterEqual(runtime.resources.usage().rgba_bytes, 4)
        self.assertEqual(edit.resources.get(draft).pixels.rgba, b"abcd")
        backend = weakref.ref(runtime._backend)
        del image, unused, edit, session, runtime
        gc.collect()
        self.assertIsNone(backend(), "A final Python owner still retains the native host")
        reopened = self.runtime("local").sessions.open(saved.manifest).wait()
        self.assertEqual(reopened.resources.get(asset).pixels, pixels)
        with self.assertRaises(zipzi.SdkFailure) as unsaved:
            reopened.resources.get(draft)
        self.assertEqual(unsaved.exception.code, "MISSING_IMAGE")

    def test_connected_close_releases_client_leases_and_recovers_lost_ack(self):
        for lost in (None, "release_client"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                with session.edit("formal") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                image = runtime.resources.prepare_image(1, 1, b"rgba")
                edit = session.edit("discard")
                asset = edit.resources.create_image("Draft", image)
                plan = edit.resources.plan(resources.RenameIntent(asset, "Draft", "Planned"))
                observer = zipzi.connect(endpoint=runtime._backend._endpoint)
                self.addCleanup(observer.close)
                if lost:
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.close()
                    request_id = runtime._backend._next
                    self.assertIsNone(error.exception.recover())
                    self.assertEqual(runtime._backend._next, request_id)
                else:
                    runtime.close()
                runtime.close()
                usage = observer.resources.usage()
                self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes), (0, 0, 0))
                observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
                retained = zipzi.Session(observer, session._token)
                self.assertEqual(retained.meshes.get(face).name, "Face")
                self.assertEqual(retained.info().history, (1, 0))
                self.assertEqual(plan.report.source.sequence, 1)

    def test_failed_terminal_reply_recovery_caches_original_error(self):
        for fault, code in (("fail_next", "EXECUTOR_FAILURE"), ("panic_next", "WORKER_PANIC")):
            with self.subTest(fault=fault):
                runtime = self.runtime("connected", drop="job")
                session = runtime.sessions.create(uid(1))
                self.control(runtime, fault)
                job = session.project.save(self.folder / fault)
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    job.wait()
                with self.assertRaises(zipzi.SdkFailure) as failed:
                    lost.exception.recover()
                self.assertEqual(failed.exception.code, code)
                with self.assertRaises(zipzi.SdkFailure) as again:
                    job.wait()
                self.assertIs(again.exception, failed.exception)

    def test_connected_close_does_not_cancel_entered_publication(self):
        runtime = self.runtime("connected")
        session = runtime.sessions.create(uid(1))
        with session.edit("base") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        observer = zipzi.connect(endpoint=runtime._backend._endpoint)
        self.addCleanup(observer.close)
        observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
        retained = zipzi.Session(observer, session._token)
        self.control(runtime, "hold_io")
        session.project.save(self.folder / "detached-save")
        self.await_io(runtime)
        runtime.close()
        self.control(observer, "release_io")
        deadline = time.monotonic() + 5
        while retained.info().modified:
            self.assertLess(time.monotonic(), deadline)
            time.sleep(.005)
        self.assertTrue((self.folder / "detached-save/project.zipzi.json").is_file())
        self.assertEqual(retained.meshes.get(face).name, "Face")
        self.assertEqual(retained.info().history, (1, 0))

    def test_confirmed_host_busy_commit_keeps_candidate_retryable(self):
        session = self.runtime("connected", settings={"ZIPZI_TEST_BUSY_COMMIT_ONCE": "1"}).sessions.create(uid(1))
        edit = session.edit("busy admission")
        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        source = edit.meshes.get(face).source
        with self.assertRaises(zipzi.Busy):
            edit.commit()
        self.assertEqual(edit.meshes.get(face).source, source)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_formal_plan_rejects_other_session_and_undo_redo_versions(self):
        for mode in ("local", "connected"):
            for changed in ("other_session", "undo", "redo"):
                with self.subTest(mode=mode, changed=changed):
                    runtime = self.runtime(mode)
                    session = runtime.sessions.create(uid(1))
                    with session.edit("base") as edit:
                        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    with session.meshes.plan(meshes.RenameIntent(face, "Face", "Planned")) as plan:
                        source = plan.report.source
                        target = runtime.sessions.create(uid(2)) if changed == "other_session" else session
                        if changed in {"undo", "redo"}:
                            session.undo().wait()
                        if changed == "redo":
                            session.redo().wait()
                        edit = target.edit("stale formal")
                        with self.assertRaises(zipzi.SdkFailure) as error:
                            edit.apply_plan(plan)
                        self.assertEqual(error.exception.code, "STALE_PLAN")
                        self.assertEqual(plan.report.source, source)
                        self.assertEqual(session.info().history, (0, 1) if changed == "undo" else (1, 0))
                        if changed != "undo":
                            self.assertEqual(session.meshes.get(face).name, "Face")

    def test_old_exception_cannot_consume_a_later_request_from_the_same_owner(self):
        from zipzi.backends import connected
        exchange = connected._exchange
        def lose_reply(stream, body):
            exchange(stream, body)
            raise EOFError("Lost after actual execution")
        for operation in ("create", "info", "apply"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected")
                session = runtime.sessions.create(uid(1))
                edit = session.edit("same owner") if operation == "apply" else None
                if edit is not None:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                def call(n):
                    if operation == "create":
                        return runtime.sessions.create(uid(n))
                    if operation == "info":
                        return session.info()
                    return edit.meshes.rename(face, f"Name {n}")
                with patch.object(connected, "_exchange", side_effect=lose_reply):
                    with self.assertRaises(zipzi.OutcomeUnknown) as first:
                        call(2)
                first.exception.recover()
                with patch.object(connected, "_exchange", side_effect=lose_reply):
                    with self.assertRaises(zipzi.OutcomeUnknown) as second:
                        call(3)
                with self.assertRaises(zipzi.SdkFailure) as resolved:
                    first.exception.recover()
                self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")
                result = second.exception.recover()
                if edit is not None:
                    self.assertEqual(result.source.sequence, 3)
                    edit.commit().wait()
                    self.assertEqual(session.meshes.get(face).name, "Name 3")

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
