import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("animation_report", Path(__file__).with_name("animation-trace-report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def event(id, stage, at, **fields):
    return dict(type="event", id=id, stage=stage, at_ns=at*1_000_000, fields=fields)


class AnimationTraceTests(unittest.TestCase):
    def test_correlates_queues_and_keeps_coalesced_requests(self):
        events = [event(10,"animation_workload",0,model=0,phase=1),
                  event(1,"animation_enqueued",1), event(1,"animation_dequeued",2),
                  event(2,"animation_enqueued",3), event(1,"animation_evaluate_begin",4),
                  event(2,"animation_coalesced",5), event(3,"animation_enqueued",6),
                  event(3,"animation_dequeued",7), event(1,"animation_evaluate_end",8),
                  event(1,"enqueued",9), event(1,"dequeued",10),event(1,"worker_begin",11),
                  event(1,"preview_submitted",12),event(1,"accepted",14),
                  event(3,"animation_cancelled",15)]
        result = report.report(list(reversed(events))+[dict(type="end",lost_events=0)])
        self.assertEqual((result["cpu_pending_high_water"],result["render_pending_high_water"]),(1,1))
        self.assertEqual(result["playback"]["0"]["requests"],3)
        self.assertEqual(result["outcomes_all_phases"],dict(accepted=1,animation_coalesced=1,animation_cancelled=1))
        self.assertEqual(result["playback"]["0"]["enqueue_to_accept_ms"]["max"],13)
        self.assertEqual(result["playback"]["0"]["evaluate_ms"]["max"],4)

    def test_presented_textures_follow_publication_lifetimes(self):
        events = [event(10,"animation_workload",0,model=0,phase=1),
                  event(1,"animation_enqueued",1), event(1,"published",8,texture=7),
                  event(20,"compose_surface",9,texture=7), event(20,"presented",12),
                  event(2,"animation_enqueued",13), event(2,"published",14,texture=7),
                  event(21,"compose_surface",15,texture=7), event(21,"presented",16)]
        result = report.report(events+[dict(type="end",lost_events=0)])
        self.assertEqual(result["playback"]["0"]["presented"],2)
        self.assertEqual(result["playback"]["0"]["presented_interval_ms"]["p50"],4)

    def test_refuses_lost_or_unfinished_trace_and_excess_cache(self):
        for records in [[], [dict(type="end",lost_events=1)],
                        [event(1,"animation_enqueued",0), event(1,"animation_cache",1,bytes=1025,budget=1024),dict(type="end",lost_events=0)]]:
            with self.assertRaises(ValueError):
                report.report(records)

    def test_refuses_failed_gpu_commands_and_native_runs(self):
        for stage in ("preview_complete", "compose_complete", "offscreen_complete"):
            with self.assertRaises(ValueError):
                report.report([event(1, stage, 0, status=5), dict(type="end", lost_events=0)])
        with self.assertRaises(ValueError):
            report.report([event(1, "benchmark_failed", 0), dict(type="end", lost_events=0)])


if __name__ == "__main__":
    unittest.main()
