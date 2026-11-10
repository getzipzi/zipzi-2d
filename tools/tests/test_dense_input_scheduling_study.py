import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "dense_input_study", Path(__file__).resolve().parents[1]
    / "benchmarks/dense-input-scheduling-study.py")
study = importlib.util.module_from_spec(spec)
spec.loader.exec_module(study)


class DenseInputStudy(unittest.TestCase):
    def test_startup_burst_can_add_latency_without_saving_steady_work(self):
        period = 1000 / 60
        inputs = sorted([1 + i * period for i in range(120)] + [1.01, 1.02])
        policies = study.synthetic_case("burst", inputs)["policies"]
        leading, continuous = policies["leading"], policies["continuous"]
        self.assertEqual(leading["apply_per_second"], continuous["apply_per_second"])
        self.assertEqual(leading["applied_sample_wait_ms"]["p95"], 0)
        self.assertGreater(continuous["applied_sample_wait_ms"]["p95"], 15)

    def test_latest_sample_wait_can_hide_held_pose_age(self):
        inputs = [1 + i * 1000 / 240 for i in range(480)]
        policies = study.synthetic_case("dense", inputs)["policies"]
        leading, continuous = policies["leading"], policies["continuous"]
        self.assertAlmostEqual(leading["applied_sample_wait_ms"]["p95"],
                               continuous["applied_sample_wait_ms"]["p95"])
        self.assertGreater(continuous["wall_sampled_applied_age_ms"]["mean"],
                           leading["wall_sampled_applied_age_ms"]["mean"] + 3)

    def test_trace_attribution_keeps_deferred_work_and_excludes_warmup(self):
        records = [{"type": "metadata", "samples": 2, "workloads": [5]}]

        def event(time, key, stage, **fields):
            records.append(dict(type="event", at_ns=time * 1_000_000,
                                id=key, stage=stage, fields=fields))

        event(0, 1, "benchmark_tick", mode=5, warmup=1)
        event(1, 1, "editor.snapshot", inclusive_ns=100_000_000)
        event(2, 2, "benchmark_tick", mode=5, warmup=0)
        event(2, 3, "input_begin", parent=2)
        event(3, 2, "input_end")
        event(10, 4, "scene_begin", input=3)
        event(12, 4, "scene_end")
        event(12, 4, "request")
        event(14, 5, "benchmark_tick", mode=5, warmup=0)
        event(14, 6, "input_begin", parent=5)
        event(14, 6, "mesh_move_immediate")
        event(14, 7, "scene_begin", input=6)
        event(16, 7, "scene_end")
        event(16, 7, "request")
        event(17, 5, "input_end")
        event(18, 8, "benchmark_done")
        records.append({"type": "end", "lost_events": 0})
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.jsonl"
            path.write_text("\n".join(map(json.dumps, records)), encoding="utf-8")
            result = study.read_trace(path)["modes"][5]
        self.assertNotIn("editor.snapshot", result["durations"])
        self.assertEqual(result["durations"]["input_sync_ms"]["mean"], 2)
        self.assertEqual(result["durations"]["requested_scene_ms"]["mean"], 2)
        self.assertEqual(result["tail_to_leading_gap_ms"]["min"], 2)
        self.assertEqual(result["tail_to_leading_under_4ms"], 1)


if __name__ == "__main__":
    unittest.main()
