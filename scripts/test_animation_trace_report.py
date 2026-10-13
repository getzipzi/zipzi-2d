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

    def test_correlates_cpu_accounting_composition_and_long_intervals(self):
        events = [event(100, "animation_workload", 0, model=0, phase=1)]
        for i, presented in enumerate([20, 28, 36, 52], start=1):
            events += [event(i, "animation_enqueued", i),
                       event(i, "animation_evaluate_begin", i+1),
                       event(i, "animation_cpu", i+5, cpu_ns=1_000_000),
                       event(i, "published", i+6, texture=i),
                       event(i, "accepted", i+7),
                       event(100+i, "ui_draw_begin", i+8),
                       event(100+i, "ui_draw_end", i+9),
                       event(200+i, "compose_begin", i+10, cpu_frame=100+i),
                       event(200+i, "compose_surface", i+10, texture=i),
                       event(200+i, "compose_gpu_end", i+12),
                       event(200+i, "presented", presented)]
        result = report.report(events+[dict(type="end", lost_events=0)])["playback"]["0"]
        self.assertEqual(result["evaluate_cpu_ms"]["p50"], 1)
        self.assertEqual(result["evaluate_non_cpu_ms"]["p50"], 3)
        self.assertEqual(result["accept_to_compose_ms"]["p50"], 3)
        self.assertEqual(result["ui_draw_ms"]["p50"], 1)
        self.assertEqual(result["long_present_intervals"],
                         [dict(after_ns=36_000_000, interval_ms=16, request_ids=[4])])

    def test_shared_presentation_timestamp_does_not_inflate_fps(self):
        events = [event(10, "animation_workload", 0, model=0, phase=1)]
        for i, time in enumerate([20, 20, 30], start=1):
            events += [event(i, "animation_enqueued", i),
                       event(i, "published", i+1, texture=i),
                       event(20+i, "compose_surface", i+2, texture=i),
                       event(20+i, "presented", time)]
        result = report.report(events+[dict(type="end", lost_events=0)])["playback"]["0"]
        self.assertEqual(result["presented"], 2)
        self.assertEqual(result["presented_textures"], 3)
        self.assertEqual(result["presented_hz"], 100)

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
