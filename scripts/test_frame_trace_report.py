"""Regression tests for timing attribution, not rendering implementation."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("report", Path(__file__).with_name("frame-trace-report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def event(id, stage, ms, **fields):
    return {"type": "event", "id": id, "stage": stage, "at_ns": int(ms * 1e6), "fields": fields}


def capture(*events):
    return [{"type": "metadata", "schema": 1, "automatic": False, "debug_assertions": False},
            *events, {"type": "end", "lost_events": 0}]


def request(id, input, start, texture=123):
    return [event(input, "input_begin", start, parent=0),
            event(input, "input_end", start + 1),
            event(id, "scene_begin", start + .2, input=input),
            event(id, "scene_end", start + .3),
            event(id, "request", start + .4, revision=id, width=100, height=100),
            event(id, "published", start + 2, texture=texture),
            event(id, "accepted", start + 3)]


def composition(id, start, texture=123, present=None):
    rows = [event(id + 1, "ui_draw_begin", start - .2),
            event(id + 1, "ui_draw_end", start - .1),
            event(id, "compose_begin", start, cpu_frame=id + 1),
            event(id, "compose_surface", start + .1, texture=texture),
            event(id, "compose_submitted", start + .2)]
    if present is not None:
        rows.append(event(id, "presented", present))
    return rows


class TraceReportTest(unittest.TestCase):
    def test_out_of_order_callbacks_and_repeated_composition(self):
        raw = capture(*request(10, 1, 0), *composition(100, 4, present=10),
                      *composition(200, 15, present=20))
        raw[1:-1] = reversed(raw[1:-1])
        rows, summary = report.analyze(raw)
        self.assertEqual(rows[0]["input_to_present_ms"], 10)
        self.assertEqual(rows[0]["composition_id"], 100)
        self.assertFalse(summary["issues"])

    def test_recycled_pointer_never_relabels_old_frame(self):
        rows, _ = report.analyze(capture(*request(10, 1, 0), *composition(100, 4, present=10),
                                        *request(20, 2, 20), *composition(200, 24, present=32)))
        self.assertEqual([r["input_to_present_ms"] for r in rows], [10, 12])

    def test_coalesced_rejected_and_no_request_remain_in_denominator(self):
        raw = capture(*request(10, 1, 0), event(10, "coalesced", 4),
                      *request(20, 2, 10), event(20, "rejected", 14),
                      event(3, "input_begin", 20, parent=0),
                      event(3, "benchmark_tick", 20, mode=1, sample=0, warmup=0, timer_late_ns=0))
        rows, _ = report.analyze(raw)
        self.assertEqual({r["outcome"] for r in rows}, {"coalesced", "rejected", "no_request"})
        self.assertTrue(all("input_to_present_ms" not in r for r in rows))

    def test_nested_input_retains_benchmark_and_warmup(self):
        raw = capture(event(7, "input_begin", 0, parent=0),
                      event(7, "benchmark_tick", 0, mode=2, sample=0, warmup=1, timer_late_ns=100),
                      *request(10, 1, 1), *composition(100, 5, present=12))
        next(e for e in raw if e.get("id") == 1 and e.get("stage") == "input_begin")["fields"]["parent"] = 7
        rows, summary = report.analyze(raw)
        self.assertEqual(rows[0]["mode"], "zoom")
        self.assertEqual(rows[0]["warmup"], 1)
        self.assertEqual(summary["modes"]["zoom"]["rows"], 0)

    def test_gpu_completion_does_not_imply_presentation(self):
        rows, _ = report.analyze(capture(*request(10, 1, 0), *composition(100, 4),
                                        event(100, "compose_complete", 6, status=4)))
        self.assertEqual(rows[0]["outcome"], "submitted_not_presented")
        self.assertNotIn("input_to_present_ms", rows[0])

    def test_loss_truncation_gpu_failure_and_clock_errors_are_reported(self):
        raw = capture(*request(10, 1, 10), *composition(100, 14, present=9),
                      event(100, "compose_complete", 16, status=5))
        raw[-1]["lost_events"] = 1
        _, summary = report.analyze(raw)
        self.assertTrue(any("lost" in s for s in summary["issues"]))
        self.assertTrue(any("negative" in s for s in summary["issues"]))
        self.assertTrue(any("GPU failure" in s for s in summary["issues"]))
        _, summary = report.analyze(raw[:-1])
        self.assertTrue(any("truncated" in s for s in summary["issues"]))

    def test_wait_overlap_uses_union_and_exact_wait_boundary(self):
        self.assertEqual(report.merge_intervals([(2, 8), (1, 3), (5, 6), (10, 12)]), [(1, 8), (10, 12)])
        self.assertEqual(report.overlap_ms(2, 11, [(1, 8), (10, 12)]), 7 / 1e6)
        raw = capture(*request(10, 1, 0), event(10, "ui_received", 4),
                      *composition(100, 4, present=10),
                      event(100, "drawable_wait_begin", 4.5), event(100, "drawable_acquired", 5),
                      event(200, "compose_begin", 1.5), event(200, "drawable_wait_begin", 2.5),
                      event(200, "drawable_acquired", 3.5))
        rows, summary = report.analyze(raw)
        self.assertEqual(rows[0]["delivery_drawable_overlap_ms"], 1)
        self.assertEqual(rows[0]["drawable_wait_ms"], .5)
        self.assertEqual(summary["modes"]["manual"]["delivery_drawable_overlap_fraction"], .5)

    def test_parameter_only_capture_requires_only_the_slider_workload(self):
        raw = capture(*request(10, 1, 0),
                      event(1, "benchmark_tick", 0, mode=4, sample=0, warmup=0, timer_late_ns=0),
                      *composition(100, 4, present=10), event(200, "benchmark_done", 20))
        raw[0].update(automatic=True, parameters_only=True, samples=1)
        _, summary = report.analyze(raw)
        self.assertFalse(summary["issues"])
        self.assertEqual(summary["modes"]["parameter_slider"]["outcomes"], {"presented": 1})

    def test_quantiles_keep_outliers(self):
        self.assertEqual(report.quantiles([1] * 99 + [40])["max"], 40)
        self.assertEqual(report.quantiles([1] * 99 + [40])["p99"], 1)
        self.assertEqual(report.quantiles([]), {"count": 0})


if __name__ == "__main__":
    unittest.main()
