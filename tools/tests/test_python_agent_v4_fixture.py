"""The v4 verifier requires independent observations, never success declarations."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "v4_fixture", ROOT / "tools/python/python_agent_v4_fixture.py"
)
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


def success(path, result, **extra):
    return dict(path=path, result=result, **extra)


class ObservationCriteria(unittest.TestCase):
    def test_image_metadata_rejects_transposed_dimensions_and_wrong_identity(self):
        from copy import deepcopy

        data = {"asset_id": "asset", "width": 768, "height": 512}
        asset = {
            "id": "asset",
            "name": "Agent texture",
            "content": {
                "kind": "image",
                "width": 768,
                "height": 512,
                "resource": "version",
            },
        }
        self.assertTrue(fixture.asset_metadata_matches(asset, data))
        transposed = deepcopy(asset)
        transposed["content"].update(width=512, height=768)
        self.assertFalse(fixture.asset_metadata_matches(transposed, data))
        self.assertFalse(
            fixture.asset_metadata_matches(dict(asset, name="Wrong label"), data)
        )
        self.assertFalse(fixture.asset_metadata_matches(dict(asset, id="other"), data))

    def test_inventory_pages_must_cover_one_source(self):
        data = {"expected_parameter_count": 3}

        def page(source, ids):
            return success(
                "Session.query",
                {
                    "source": source,
                    "value": {
                        "records": [{"address": {"id": identity}} for identity in ids],
                        "next_offset": 2,
                    },
                },
            )

        first = page({"revision": 1}, ["a", "b"])
        repeated = page({"revision": 1}, ["a", "b"])
        stale = page({"revision": 2}, ["c"])
        coherent = page({"revision": 1}, ["c"])
        check = lambda events: fixture.observation_checks("A01", data, events)[
            "observed_consistent_complete_scan"
        ]
        self.assertFalse(check([]))
        self.assertFalse(check([first, repeated]))
        self.assertFalse(check([first, stale]))
        self.assertTrue(check([first, coherent]))

    def test_image_bytes_require_bounded_real_uploads_and_reads(self):
        data = {"width": 768, "height": 512}
        chunk = {"length": 786432, "sha256": "captured-by-observer"}
        events = [success("Session.undo", {}), success("Session.redo", {})]
        events += [
            success("Blob.append", 786432, arguments={"args": [0, chunk]})
            for _ in range(4)
        ]
        events += [success("Blob.read", chunk) for _ in range(4)]
        self.assertTrue(all(fixture.observation_checks("A02", data, events).values()))
        missing_reads = [event for event in events if event["path"] != "Blob.read"]
        self.assertFalse(
            fixture.observation_checks("A02", data, missing_reads)[
                "observed_bounded_multi_chunk_readback"
            ]
        )
        oversized = [
            success("Blob.read", {"length": fixture.CHUNK + 1, "sha256": "oversized"})
        ]
        self.assertFalse(
            fixture.observation_checks("A02", data, events + oversized)[
                "observed_bounded_multi_chunk_readback"
            ]
        )

    def test_sampling_observations_must_be_bracketed_by_actual_session_reads(self):
        data = {"parameter_id": "p", "clip_id": "clip", "times": [0.0, 1.0]}
        info = {"version": {"revision": 7}, "history": {"undo_steps": 2}}
        report = {
            "frames": [
                {
                    "animation": {
                        "time": requested,
                        "parameters": {"p": -20 + 40 * local},
                        "motion_positions": [
                            {"motion_id": "clip", "local_time": local}
                        ],
                    }
                }
                for requested, local in ((0.0, 0.0), (1.0, 1 - 1 / 60))
            ]
        }
        before = success("Session.info", info)
        call = success("Session.samples", {"id": "job"})
        read = success(
            "Blob.read", {"json": report, "length": 100, "sha256": "captured"}
        )
        after = success("Session.info", dict(info))
        actual = fixture.observation_checks("A03", data, [before, call, read, after])
        self.assertTrue(actual["observed_real_sample_values"])
        self.assertTrue(actual["observed_read_only_sampling"])
        # A complete observation may be attached to the final small read; that
        # read's own bytes do not have to form a JSON document.
        final_chunk = success(
            "Blob.read",
            {"length": 7, "sha256": "last-chunk"},
            readback={"json": report, "length": 100, "session": "session"},
        )
        self.assertTrue(
            all(
                fixture.observation_checks(
                    "A03", data, [before, call, final_chunk, after]
                ).values()
            )
        )
        self.assertFalse(
            fixture.observation_checks("A03", data, [before, after, call, read])[
                "observed_read_only_sampling"
            ]
        )
        changed = success(
            "Session.info", {"version": {"revision": 8}, "history": info["history"]}
        )
        self.assertFalse(
            fixture.observation_checks("A03", data, [before, call, read, changed])[
                "observed_read_only_sampling"
            ]
        )
        report["frames"][1]["animation"]["motion_positions"][0]["local_time"] = 1.0
        self.assertFalse(fixture.playback_matches(report, data))

    def test_recovery_requires_real_errors_and_job_wait(self):
        data = {"recovered_id": "recovered"}
        source = {
            "draft": "draft",
            "base": {"session": "session", "revision": 1},
            "sequence": 0,
        }
        completed = dict(source, sequence=1)
        declared = [
            {
                "path": "agent_report",
                "result": {"stale": True, "draft_ended": True, "completed": True},
            }
        ]
        self.assertFalse(
            any(fixture.observation_checks("A04", data, declared).values())
        )
        events = [
            {"path": "Session.query", "error": {"code": "STALE_SOURCE"}},
            {
                "path": "Draft.apply",
                "error": {"code": "DUPLICATE_ID"},
                "state": "ended",
            },
            success(
                "Draft.apply_async",
                {"id": "job"},
                source=source,
                arguments={
                    "commands": [
                        {"command": "create_parameter", "spec": {"id": "recovered"}}
                    ]
                },
            ),
            {
                "path": "Job.wait",
                "owner": "job",
                "arguments": {"timeout": 0},
                "error": {"code": "JOB_TIMEOUT"},
            },
            success("Job.wait", completed, owner="job"),
            success(
                "Draft.publish",
                {"version": {"revision": 2}},
                source=completed,
                state="ended",
            ),
        ]
        self.assertTrue(all(fixture.observation_checks("A04", data, events).values()))
        self.assertFalse(
            fixture.observation_checks("A04", data, events[:-1])[
                "observed_async_completion"
            ]
        )
        foreign_waits = [
            dict(e, owner="save-job") if e["path"] == "Job.wait" else e for e in events
        ]
        self.assertFalse(
            fixture.observation_checks("A04", data, foreign_waits)[
                "observed_zero_timeout_wait"
            ]
        )
        self.assertFalse(
            fixture.observation_checks("A04", data, foreign_waits)[
                "observed_async_completion"
            ]
        )
        foreign_publish = [
            *events[:-1],
            dict(events[-1], source=dict(completed, draft="other-draft")),
        ]
        self.assertFalse(
            fixture.observation_checks("A04", data, foreign_publish)[
                "observed_async_completion"
            ]
        )
        wrong_command = dict(data, recovered_id="another-parameter")
        self.assertFalse(
            fixture.observation_checks("A04", wrong_command, events)[
                "observed_async_completion"
            ]
        )
        events[1]["error"]["code"] = "DRAFT_BUSY"
        self.assertFalse(
            fixture.observation_checks("A04", data, events)[
                "observed_ordinary_failed_draft_ended"
            ]
        )


if __name__ == "__main__":
    unittest.main()
