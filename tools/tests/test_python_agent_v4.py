"""Exercise real packet isolation; no SDK tasks are graded by agent declarations."""

import importlib.util
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/python"))
SPEC = importlib.util.spec_from_file_location(
    "agent_v4_harness", ROOT / "tools/python/python_agent_v4.py"
)
harness = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = harness
SPEC.loader.exec_module(harness)


class ObservationTransport(unittest.TestCase):
    def test_unicode_output_respects_delivered_byte_limit(self):
        request = {
            "id": 1,
            "method": "tools/call",
            "params": {"arguments": {"command": "probe"}},
        }
        output = io.StringIO()
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(sys, "stdin", io.StringIO(json.dumps(request) + "\n")),
            patch.object(
                harness,
                "execute",
                return_value={
                    "command": "probe",
                    "returncode": 0,
                    "elapsed_seconds": 0,
                    "output": "中文" * 10000,
                },
            ),
            redirect_stdout(output),
        ):
            harness.serve(Path(folder), Path(folder))
        delivered = json.loads(output.getvalue())["result"]["content"][0]["text"]
        self.assertLessEqual(len(delivered.encode()), 16384)
        self.assertFalse(json.loads(delivered)["output_complete"])

    def test_sample_reads_cover_exact_blob_without_gaps(self):
        observer = harness.SampleReadbacks()
        payload = json.dumps({"frames": [{"label": "样本"}]}).encode()
        observer.observe(
            {"path": "Session.samples", "owner": "session", "result": {"id": "job"}},
            None,
        )
        observer.observe(
            {
                "path": "Job.blob",
                "owner": "job",
                "result": {"id": "blob", "length": len(payload)},
            },
            None,
        )

        def read(owner, offset, chunk):
            event = {
                "path": "Blob.read",
                "owner": owner,
                "arguments": {"offset": offset},
            }
            observer.observe(event, chunk)
            return event

        self.assertNotIn("readback", read("unrelated", 0, payload))
        self.assertNotIn("readback", read("blob", 10, payload[10:]))
        self.assertNotIn("readback", read("blob", 0, payload[:5]))
        self.assertNotIn("readback", read("blob", 0, payload[:2]))
        complete = read("blob", 5, payload[5:10])["readback"]
        self.assertEqual(complete["json"], json.loads(payload))
        self.assertEqual(complete["session"], "session")
        self.assertEqual(complete["length"], len(payload))
        self.assertNotIn("blob", observer.blobs)


@unittest.skipUnless(sys.platform == "darwin", "Seatbelt isolation is macOS-specific")
class PacketIsolation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zipzi-v4-isolation-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.packet, self.private = self.root / "packet", self.root / "private"
        for path in (
            self.packet / "work",
            self.packet / "output",
            self.packet / "host",
            self.packet / "environment/bin",
            self.private,
        ):
            path.mkdir(parents=True)
        (self.packet / "environment/bin/python").symlink_to(sys.executable)
        (self.private / "secret").write_text("judge-only")
        (self.packet / "environment/package.txt").write_text("installed")
        harness.install_workflow_runner(self.packet)

    def call(self, command):
        return harness.execute(command, self.packet, self.private, 1)

    def test_repository_and_private_state_are_unreadable_but_work_is_writable(self):
        for path in (ROOT / "AGENTS.md", self.private / "secret"):
            result = self.call("cat " + shlex.quote(str(path)))
            self.assertNotEqual(result["returncode"], 0, result)
        result = self.call("echo task > work/task.txt && cat work/task.txt")
        self.assertEqual(result["returncode"], 0, result)
        self.assertEqual(result["output"].strip(), "task")

    def test_package_is_readable_but_cannot_be_changed(self):
        result = self.call("cat environment/package.txt")
        self.assertEqual(result["returncode"], 0, result)
        result = self.call("echo changed > environment/package.txt")
        self.assertNotEqual(result["returncode"], 0, result)
        self.assertEqual(
            (self.packet / "environment/package.txt").read_text(), "installed"
        )

    def test_network_is_denied(self):
        command = "environment/bin/python -I -c 'import socket; socket.create_connection((\"1.1.1.1\",443),1)'"
        result = self.call(command)
        self.assertNotEqual(result["returncode"], 0, result)
        self.assertIn("Operation not permitted", result["output"])

    def test_heredoc_uses_allowed_task_temporary_directory(self):
        result = self.call("environment/bin/python -I - <<'PY'\nprint('ready')\nPY")
        self.assertEqual(result["returncode"], 0, result)
        self.assertEqual(result["output"].strip(), "ready")

    def test_workflow_cannot_select_a_script_outside_packet_work(self):
        result = self.call("sdk-python ../private/secret")
        self.assertNotEqual(result["returncode"], 0)
        self.assertIn("under work", result["output"])

    def test_compound_workflow_retains_external_observation_channel(self):
        probe = 'import socket,sys\ns=socket.socket(fileno=int(sys.argv[1]))\ns.sendall(b\'{"path":"probe","result":1}\\n\')\ns.close()\nprint("observed")'
        with patch.object(harness, "BOOTSTRAP", probe):
            harness.install_workflow_runner(self.packet)
        result = self.call(
            "cat > work/run.py <<'PY'\nprint('workflow')\nPY\nsdk-python work/run.py"
        )
        self.assertEqual(result["returncode"], 0, result)
        self.assertEqual(result["output"].strip(), "observed")
        events = [
            json.loads(line)
            for line in (self.private / "observations.ndjson").read_text().splitlines()
        ]
        self.assertEqual(events, [{"path": "probe", "result": 1, "shell_call": 1}])

    def test_stopping_controller_also_terminates_independent_shell(self):
        code = (
            "import sys; from pathlib import Path; sys.path.insert(0, sys.argv[1]); "
            "from python_agent_v4 import execute; "
            "execute(\"environment/bin/python -I -c 'import time;time.sleep(30)'\", "
            "Path(sys.argv[2]), Path(sys.argv[3]), 1)"
        )
        process = subprocess.Popen(
            [
                sys.executable,
                "-c",
                code,
                str(ROOT / "tools/python"),
                str(self.packet),
                str(self.private),
            ],
            start_new_session=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            record = self.private / "active-shell-pgid"
            deadline = time.monotonic() + 10
            while (
                not record.exists()
                and process.poll() is None
                and time.monotonic() < deadline
            ):
                time.sleep(0.02)
            self.assertTrue(record.exists())
            pid = int(record.read_text())
            self.assertNotEqual(os.getpgid(pid), os.getpgid(process.pid))
            harness.stop_participant(process, self.private)
            self.assertIsNotNone(process.poll())
            self.assertFalse(record.exists())
            self.assertFalse(
                harness.psutil.pid_exists(pid)
                and harness.psutil.Process(pid).status() != harness.psutil.STATUS_ZOMBIE
            )
        finally:
            harness.stop_participant(process, self.private)


if __name__ == "__main__":
    unittest.main()
