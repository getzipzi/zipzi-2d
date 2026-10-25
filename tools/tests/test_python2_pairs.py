"""Counterbalanced P0 summaries retain the original noise/resource policy."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/python"))
import python2_pairs as pairs
SPEC = importlib.util.spec_from_file_location("paired_measurements", ROOT / "crates/zipzi-python2/tools/baseline.py")
measurements = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measurements)


class Python2Pairs(unittest.TestCase):
    def runs(self):
        result = {}
        for mode in ("local", "connected"):
            probe = dict(rust=dict(enabled=True, rust_live_bytes=110, rust_start_bytes=100, rust_peak_bytes=160,
                rust_allocations=10, rust_allocated_bytes=100, json_encodes=4 if mode == "connected" else 0,
                json_decodes=4 if mode == "connected" else 0, json_value_builds=0, json_admission_encodes=1,
                materialized_input_bytes=16, copied_payload_bytes=64), image_memory_after=dict(
                    prepared_images=0, plans=0, preparing_bytes=0), calls={"request": 1})
            identity = dict(validated=True, extension_sha256="native", host_sha256="host", client_sha256="client",
                            measurement_sha256="workload", platform="test", python="3.14")
            timing = dict(identity, cases={case: dict(wall_ns=list(range(1, 33))) for case in measurements.CASES})
            counters = dict(identity, cases={case: dict(probe=deepcopy(probe)) for case in measurements.CASES})
            result[mode] = {"timing": [deepcopy(timing) for _ in range(5)],
                            "probe": [deepcopy(counters) for _ in range(5)]}
        return result

    def policy(self):
        return {"thresholds": {"p50_ratio": 1.5, "p95_ratio": 2.0, "max_process_p50_range": 0.5,
            "copy_growth_bytes": 0, "wire_poll_codec_allowance": 3,
            "max_rust_peak_growth_ratio": 1.5, "frozen": True}}

    def test_order_counterbalances_without_dropping_any_side(self):
        self.assertEqual([pairs.pair_order(i) for i in range(3)], [
            ("baseline", "candidate"), ("candidate", "baseline"), ("baseline", "candidate")])

    def test_summary_retains_separate_probes_samples_and_unchanged_thresholds(self):
        report = pairs.summarize(measurements, self.runs(), self.policy(), {})
        self.assertEqual(report["samples"], 32)
        self.assertEqual(report["warmup"], 4)
        self.assertEqual(report["runs"], 5)
        self.assertEqual(report["thresholds"], self.policy()["thresholds"])
        self.assertEqual(report["measurements"]["local"]["image"]["p50_ns"], 16.5)
        self.assertEqual(len(report["measurements"]["connected"]["plan"]["probes"]), 5)
        self.assertEqual(measurements.compare(report, deepcopy(report)), [])
        changed = deepcopy(report)
        changed["measurements"]["connected"]["plan"]["probes"][0]["rust"]["copied_payload_bytes"] = 65
        self.assertIn("connected.plan.copied_payload_bytes", measurements.compare(changed, report))

    def test_missing_workloads_or_fabricated_probes_fail_and_noise_is_not_hidden(self):
        missing = self.runs()
        missing["local"]["timing"][0]["cases"].pop("image")
        with self.assertRaises(ValueError):
            pairs.summarize(measurements, missing, self.policy(), {})
        corrupt = self.runs()
        corrupt["connected"]["probe"][0]["cases"]["image"]["probe"]["rust"]["enabled"] = False
        with self.assertRaises(ValueError):
            pairs.summarize(measurements, corrupt, self.policy(), {})
        noisy = self.runs()
        noisy["local"]["timing"][0]["cases"]["image"]["wall_ns"] = [1000 + n for n in range(32)]
        report = pairs.summarize(measurements, noisy, self.policy(), {})
        self.assertFalse(report["thresholds"]["frozen"])
        self.assertIn("local.image", report["noise_rejections"])


if __name__ == "__main__":
    unittest.main()
