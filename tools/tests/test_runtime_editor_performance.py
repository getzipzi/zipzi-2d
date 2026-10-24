"""Keep the predeclared timing gates independent for every operation."""
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools/runtime'))
from editor_benchmark_report import compare


def runs(cpu, display):
    return [dict(cpu=dict(p50=cpu, p95=cpu, kind='cpu'), display=dict(p50=display, p95=display, kind='display')) for _ in range(5)]


class PerformanceGates(unittest.TestCase):
    def test_absolute_margin_and_percentage_are_maximum_not_additive(self):
        self.assertTrue(compare(runs(1, 1), runs(1.2, 2))['passed'])
        self.assertFalse(compare(runs(1, 1), runs(1.21, 2))['passed'])
        self.assertTrue(compare(runs(10, 10), runs(11, 11))['passed'])
        self.assertFalse(compare(runs(10, 10), runs(11.01, 11))['passed'])

    def test_slow_tail_cannot_hide_in_other_operations_or_median(self):
        candidate = runs(1, 1)
        for sample in candidate:
            sample['cpu']['p95'] = 1.21
            sample['display']['p50'] = 0.1
        self.assertFalse(compare(runs(1, 1), candidate)['measurements']['cpu']['p95']['passed'])

    def test_requires_five_consistent_pairs(self):
        with self.assertRaises(ValueError):
            compare(runs(1, 1)[:4], runs(1, 1)[:4])
        candidate = runs(1, 1)
        del candidate[2]['cpu']
        with self.assertRaises(ValueError):
            compare(runs(1, 1), candidate)

class MeasuredInputs(unittest.TestCase):
    def test_startup_and_idle_frames_are_not_measured_input_latency(self):
        from editor_benchmark_report import normalized
        sample = dict(count=30, p50=1., p95=2.)
        rows = dict(metrics_ms=dict(input_handler_ms=sample, input_to_present_ms=sample))
        result = normalized('deformations', dict(modes=dict(manual=rows, mesh_multi_vertex=rows)))
        self.assertEqual(set(result), {'deformations/mesh_multi_vertex/input_handler_ms', 'deformations/mesh_multi_vertex/input_to_present_ms'})


if __name__ == '__main__':
    unittest.main()
