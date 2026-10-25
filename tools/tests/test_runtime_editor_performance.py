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
    def test_pairs_balance_order_without_omitting_either_executable(self):
        from editor_benchmark_pairs import pair_order
        orders = [pair_order(index) for index in range(20)]
        self.assertTrue(all(set(order) == {'baseline', 'candidate'} and len(order) == 2 for order in orders))
        for position in (0, 1):
            self.assertEqual(sum(order[position] == 'baseline' for order in orders), 10)
            self.assertEqual(sum(order[position] == 'candidate' for order in orders), 10)

    def test_measured_processes_cannot_reuse_the_desktop_runtime_owner(self):
        from editor_benchmark_pairs import isolated_runtime
        parent = dict(ZIPZI_RUNTIME_DIR='/user/runtime', ZIPZI_RUNTIME_PROFILE='user',
                      ZIPZI_RUNTIME_OWNER_FD='42', ZIPZI_RUNTIME_INTERACTION_SAMPLES='60')
        with isolated_runtime(parent) as first, isolated_runtime(parent) as second:
            self.assertNotEqual(first['ZIPZI_RUNTIME_DIR'], second['ZIPZI_RUNTIME_DIR'])
            directory = Path(first['ZIPZI_RUNTIME_DIR'])
            self.assertTrue(directory.is_dir())
            self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
            self.assertLess(len(str(directory / 'benchmark.sock').encode()), 104)
            self.assertEqual(first['ZIPZI_RUNTIME_PROFILE'], 'benchmark')
            self.assertNotIn('ZIPZI_RUNTIME_OWNER_FD', first)
            self.assertEqual(first['ZIPZI_RUNTIME_INTERACTION_SAMPLES'], '60')
        self.assertFalse(directory.exists())
        self.assertEqual(parent['ZIPZI_RUNTIME_OWNER_FD'], '42')

    def test_startup_and_idle_frames_are_not_measured_input_latency(self):
        from editor_benchmark_report import normalized
        sample = dict(count=30, p50=1., p95=2.)
        rows = dict(metrics_ms=dict(input_handler_ms=sample, input_to_present_ms=sample))
        result = normalized('deformations', dict(modes=dict(manual=rows, mesh_multi_vertex=rows)))
        self.assertEqual(set(result), {'deformations/mesh_multi_vertex/input_handler_ms', 'deformations/mesh_multi_vertex/input_to_present_ms'})


if __name__ == '__main__':
    unittest.main()
