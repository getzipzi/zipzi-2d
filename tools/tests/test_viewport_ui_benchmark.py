"""Guard isolation and completeness in native/fallback viewport comparisons."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location('viewport_benchmark', Path(__file__).resolve().parents[1]/'benchmarks/viewport-ui-benchmark.py')
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class ViewportBenchmarkTest(unittest.TestCase):
    def test_isolation_scrubs_resident_host_and_renderer_overrides(self):
        args = SimpleNamespace(model=Path('/model'), samples=360, warmup=30, hz=120, detail=False, warp='warp')
        base = dict(PATH='/bin', ZIPZI_RUNTIME_OWNER_FD='99', ZIPZI_DEV_SESSION_DIR='/live',
                    ZIPZI_TRACE_BATCH_MESH_EDGES='0', GPUI_TRACE_DISPLAY_SYNC='0',
                    ZIPZI_BENCH_PARAMETERS='1', MTL_HUD_ENABLED='1')
        native = report.environment(base, args, Path('/output'), '/tmp/private', 'deformations', 'native')
        for key in ('ZIPZI_RUNTIME_OWNER_FD', 'ZIPZI_DEV_SESSION_DIR', 'ZIPZI_TRACE_BATCH_MESH_EDGES',
                    'GPUI_TRACE_DISPLAY_SYNC', 'ZIPZI_BENCH_PARAMETERS', 'MTL_HUD_ENABLED'):
            self.assertNotIn(key, native)
        self.assertEqual(native['ZIPZI_RUNTIME_DIR'], '/tmp/private')
        self.assertEqual(native['ZIPZI_BENCH_WARP'], 'warp')
        gpui = report.environment(base, args, Path('/output'), '/tmp/private', 'deformations', 'gpui')
        self.assertEqual(gpui, {**native, 'ZIPZI_TRACE_BATCH_MESH_EDGES': '1'})
        self.assertEqual(base['ZIPZI_RUNTIME_OWNER_FD'], '99')

    def test_balances_position_and_refuses_wrong_renderer_or_missing_workload(self):
        self.assertEqual(report.pair_order(0), tuple(reversed(report.pair_order(1))))
        summary = {'issues': [], 'metadata': {'diagnostics': {'native_canvas_overlay': True}},
                   'modes': {'parameter_slider': {'inputs': 360}}}
        rows = [{'stage': 'canvas_overlay_draw'}]
        report.validate(rows, summary, 'parameter_selected', 'native', 360)
        summary['modes']['manual'] = {'inputs': 0}
        report.validate(rows, summary, 'parameter_selected', 'native', 360)
        with self.assertRaisesRegex(ValueError, 'renderer override'):
            report.validate(rows, summary, 'parameter_selected', 'gpui', 360)
        with self.assertRaisesRegex(ValueError, 'canvas path'):
            report.validate([], summary, 'parameter_selected', 'native', 360)
        with self.assertRaisesRegex(ValueError, 'workloads'):
            report.validate(rows, summary, 'navigation', 'native', 360)
        with self.assertRaisesRegex(ValueError, 'inputs'):
            report.validate(rows, summary, 'parameter_selected', 'native', 359)

    def test_refuses_unequal_fixture_and_failed_overlay_gpu(self):
        a = {'fixtures': [{'object': 'warp', 'component_ids': [3]}]}
        report.check_pair([a, a])
        with self.assertRaisesRegex(ValueError, 'target or component'):
            report.check_pair([a, {'fixtures': [{'object': 'warp', 'component_ids': [4]}]}])
        summary = {'issues': [], 'metadata': {'diagnostics': {'native_canvas_overlay': True}},
                   'modes': {'parameter_slider': {'inputs': 360}}}
        with self.assertRaisesRegex(ValueError, 'GPU failure'):
            report.validate([{'stage': 'canvas_overlay_draw'},
                             {'stage': 'overlay_gpu_complete', 'fields': {'status': 5}}],
                            summary, 'parameter_selected', 'native', 360)

    def test_deformer_comparison_keeps_native_backend_and_audits_actual_toggle(self):
        args = SimpleNamespace(model=Path('/model'), samples=360, warmup=30, hz=120, detail=False)
        on = report.environment({}, args, Path('/output'), '/tmp/private', 'navigation', 'on')
        off = report.environment({}, args, Path('/output'), '/tmp/private', 'navigation', 'off')
        self.assertEqual(on['ZIPZI_BENCH_REGULAR_DEFORMERS'], 'on')
        self.assertEqual(off, {**on, 'ZIPZI_BENCH_REGULAR_DEFORMERS': 'off'})
        self.assertEqual(on['ZIPZI_BENCH_NAVIGATION_ONLY'], '1')
        self.assertNotIn('ZIPZI_TRACE_BATCH_MESH_EDGES', on)
        self.assertEqual(report.pair_order(1, 'deformers'), ('off', 'on'))
        summary = {'issues': [], 'metadata': {'regular_deformers': False,
                                            'diagnostics': {'native_canvas_overlay': True}},
                   'modes': {'pan': {'inputs': 360}, 'zoom': {'inputs': 360}}}
        records = [{'stage': 'canvas_overlay_draw'}]
        report.validate(records, summary, 'navigation', 'off', 360)
        with self.assertRaisesRegex(ValueError, 'display override'):
            report.validate(records, summary, 'navigation', 'on', 360)
        a = {'fixtures': [], 'deformer-audit': {'mesh_vertices': 5, 'history': [0, 0]}}
        b = {'fixtures': [], 'deformer-audit': {'mesh_vertices': 6, 'history': [0, 0]}}
        with self.assertRaisesRegex(ValueError, 'Mesh selection or history'):
            report.check_pair([a, b])


if __name__ == '__main__':
    unittest.main()
