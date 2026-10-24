import importlib.util
from pathlib import Path
import unittest
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/benchmarks"))

spec = importlib.util.spec_from_file_location("properties_report", (Path(__file__).resolve().parents[2] / "tools").joinpath("benchmarks/properties-trace-report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def event(identity, stage, time, **fields):
    return dict(type="event", id=identity, stage=stage, at_ns=time, fields=fields)


def fixture(case="idle"):
    index = report.CASES.index(case)
    return [
        dict(type="metadata", schema=1, automatic=True, debug_assertions=False,
             properties=True, property_case=case, samples=2),
        event(1, "properties_phase", 0, case=index, active=1),
        event(2, "input_begin", 10),
        event(2, "properties_tick", 11, case=index, sample=0, warmup=0, timer_late_ns=0),
        event(2, "input_end", 20),
        event(2, "properties_checked", 21, changed=0),
        event(3, "input_begin", 30),
        event(3, "properties_tick", 31, case=index, sample=1, warmup=0, timer_late_ns=0),
        event(3, "input_end", 40),
        event(3, "properties_checked", 41, changed=0),
        event(1, "properties_phase_end", 50, case=index, changed_samples=0),
        event(4, "benchmark_done", 60),
        dict(type="end", lost_events=0),
    ]


class PropertiesTraceTests(unittest.TestCase):
    def test_explicit_suite_does_not_require_unselected_cases(self):
        records = fixture("physics_idle")
        records[0].pop("property_case")
        records[0]["property_cases"] = ["physics_idle"]
        result = report.analyze(records)[1]
        self.assertEqual(result["issues"], [])
        self.assertEqual(list(result["cases"]), ["physics_idle"])

    def test_physics_edits_require_observed_changes(self):
        result = report.analyze(fixture("physics_input_weight_text"))[1]
        self.assertIn("physics_input_weight_text: no verified value changes", result["issues"])

    def test_invalid_suite_metadata_is_rejected(self):
        for names in ([], ["missing"], ["idle", "idle"]):
            records = fixture()
            records[0]["property_cases"] = names
            with self.assertRaisesRegex(ValueError, "property_cases"):
                report.analyze(records)

    def test_chain_probes_follow_tick_identity_exclude_warmup_and_keep_noops(self):
        records = fixture() + [
            event(100, "properties_tick", 1, case=report.CASES.index("idle"), sample=0, warmup=1, timer_late_ns=0),
            event(100, "core.fork_candidate", 2, calls=99, inclusive_ns=900, self_ns=800),
            event(2, "core.fork_candidate", 19, calls=1, inclusive_ns=90, self_ns=40),
        ]
        stages = report.analyze(records)[1]["cases"]["idle"]["chain_stages"]["stages"]
        self.assertEqual(stages["core.fork_candidate"]["calls"], 1)
        self.assertEqual(stages["core.fork_candidate"]["inputs_with_calls"], 1)
        self.assertEqual(stages["core.fork_candidate"]["self_ms"]["count"], 2)
        self.assertEqual(stages["core.fork_candidate"]["inclusive_ms"]["max"], 90/1e6)

    def test_idle_does_not_require_gpu_work_or_invent_presentation_latency(self):
        rows, result = report.analyze(fixture())
        self.assertEqual(result["issues"], [])
        self.assertEqual(len(rows), 2)
        self.assertEqual(result["cases"]["idle"]["metrics_ms"]["input_to_present_ms"]["count"], 0)

    def test_handler_timing_excludes_scripted_wait_and_nested_scopes(self):
        records = fixture() + [
            event(10, "input_begin", 12, parent=2),
            event(11, "input_begin", 13, parent=10),
            event(11, "input_end", 14),
            event(10, "input_end", 16),
        ]
        result = report.analyze(records)[1]["cases"]["idle"]["metrics_ms"]
        self.assertEqual(result["native_event_handler_ms"]["count"], 1)
        self.assertEqual(result["native_event_handler_ms"]["max"], 4 / 1e6)
        self.assertEqual(result["interaction_wall_ms"]["max"], 9 / 1e6)

    def test_property_stages_keep_cpu_and_wall_separate_and_exclude_warmup(self):
        records = fixture() + [
            event(20, "property_commit", 5, wall_ns=900, cpu_ns=800),
            event(21, "property_commit", 15, wall_ns=90, cpu_ns=80),
            event(22, "property_redraw", 16, wall_ns=30, cpu_ns=20),
            event(23, "property_commit", 55, wall_ns=900, cpu_ns=800),
        ]
        metrics = report.analyze(records)[1]["cases"]["idle"]["metrics_ms"]
        self.assertEqual(metrics["property_commit_wall_ms"]["count"], 1)
        self.assertEqual(metrics["property_commit_wall_ms"]["max"], 90 / 1e6)
        self.assertEqual(metrics["property_commit_cpu_ms"]["max"], 80 / 1e6)
        self.assertEqual(metrics["property_redraw_cpu_ms"]["max"], 20 / 1e6)

    def test_missing_input_or_end_fails(self):
        records = [r for r in fixture() if r.get("stage") != "properties_checked"]
        self.assertTrue(report.analyze(records)[1]["issues"])
        self.assertTrue(report.analyze(fixture()[:-1])[1]["issues"])

    def test_unexplained_request_loss_fails_even_when_ticks_are_complete(self):
        records = fixture() + [
            event(10, "scene_begin", 13, input=2),
            event(10, "scene_end", 14),
            event(10, "request", 15, revision=1, width=100, height=100),
        ]
        self.assertIn("idle: preview requests lack terminal outcomes", report.analyze(records)[1]["issues"])

    def test_ui_change_requires_native_draw_and_presentation(self):
        result = report.analyze(fixture("picker_tabs"))[1]
        self.assertIn("picker_tabs: no measured UI draws", result["issues"])
        self.assertIn("picker_tabs: no native window presentations", result["issues"])

    def test_gpu_failure_is_not_a_pass(self):
        records = fixture() + [event(5, "compose_complete", 45, status=5)]
        self.assertTrue(any("GPU failure" in issue for issue in report.analyze(records)[1]["issues"]))

    def test_debug_and_dropped_trace_are_rejected(self):
        records = fixture()
        records[0]["debug_assertions"] = True
        records[-1]["lost_events"] = 1
        self.assertEqual(len(report.analyze(records)[1]["issues"]), 2)


if __name__ == "__main__":
    unittest.main()
