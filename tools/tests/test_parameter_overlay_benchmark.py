"""Boundary and GPU completeness checks for whole-model overlay attribution."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('overlay_benchmark', Path(__file__).resolve().parents[1]/'benchmarks/parameter-overlay-benchmark.py')
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def event(id, stage, ms, **fields):
    return dict(type='event', id=id, stage=stage, at_ns=int(ms*1e6), fields=fields)


class OverlayAttributionTest(unittest.TestCase):
    def test_only_measured_interval_contributes_to_each_stage(self):
        rows = [event(1, 'benchmark_tick', 0, warmup=1), event(2, 'benchmark_tick', 10, warmup=0),
                event(3, 'parameter_phase_end', 30)]
        for id, ms, duration in [(4, 1, 99), (5, 12, 2), (6, 20, 4), (7, 31, 99)]:
            rows += [event(id, 'overlay_build', ms, end_ns=int((ms+duration)*1e6), origin=1),
                     event(id, 'overlay.access', ms, inclusive_ns=int(duration*1e6), self_ns=100, calls=1)]
        result = report.diagnose(rows)
        self.assertEqual(result['ticks'], 1)
        self.assertEqual(result['build_ms_by_origin']['1']['count'], 2)
        self.assertEqual(result['build_sum_ms_by_origin']['1'], 6)
        self.assertEqual(result['stages']['1/overlay.access']['calls'], 2)
        self.assertEqual(result['stages']['1/overlay.access']['sum_inclusive_ms'], 6)

    def test_out_of_order_gpu_callbacks_and_cached_draws(self):
        rows = [event(1, 'benchmark_tick', 10, warmup=0), event(2, 'parameter_phase_end', 30),
                event(3, 'overlay_commit', 11, **{'pass': 1}), event(3, 'overlay_gpu_complete', 15, status=4),
                event(3, 'overlay_gpu_end', 14), event(3, 'overlay_gpu_start', 12),
                event(4, 'ui_draw_begin', 20), event(4, 'ui_draw_end', 22)]
        result = report.diagnose(list(reversed(rows)))
        self.assertEqual(result['overlay_gpu_ms_by_pass']['1']['p95'], 2)
        self.assertEqual(result['ui_draw_ms']['p95'], 2)
        self.assertEqual(result['build_ms_by_origin'], {})

    def test_incomplete_or_failed_gpu_commands_are_not_silently_dropped(self):
        rows = [event(1, 'benchmark_tick', 10, warmup=0), event(2, 'parameter_phase_end', 30),
                event(3, 'overlay_commit', 11, **{'pass': 2})]
        with self.assertRaisesRegex(ValueError, 'missing overlay GPU'):
            report.diagnose(rows)
        rows += [event(3, 'overlay_gpu_start', 12), event(3, 'overlay_gpu_end', 13),
                 event(3, 'overlay_gpu_complete', 14, status=5)]
        with self.assertRaisesRegex(ValueError, 'GPU failure'):
            report.diagnose(rows)


if __name__ == '__main__':
    unittest.main()
