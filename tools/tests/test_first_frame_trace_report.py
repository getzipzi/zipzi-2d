import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'first_frame_report', Path(__file__).parents[1] / 'benchmarks/first-frame-trace-report.py')
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def fixture():
    rows = [{'type': 'metadata', 'schema': 1, 'first_frame': True, 'automatic': True}]

    def event(identity, stage, at, **fields):
        rows.append({'type': 'event', 'id': identity, 'stage': stage,
                     'at_ns': at * 1_000_000, 'fields': fields})

    for stage, at in [('load_begin', 100), ('load_admitted', 110),
                      ('load_worker_begin', 120), ('load_worker_end', 300),
                      ('load_attach_begin', 310), ('load_attached', 335)]:
        event(1, stage, at)
    for stage, at in [('scene_begin', 320), ('scene_end', 330), ('enqueued', 340),
                      ('worker_begin', 350), ('textures_ready', 360),
                      ('model_synced', 370), ('encode_begin', 380),
                      ('preview_commit_begin', 390), ('preview_submitted', 400),
                      ('ui_received', 412), ('accepted', 415),
                      ('preview_gpu_start', 405), ('preview_gpu_end', 420)]:
        event(2, stage, at)
    event(2, 'request', 331, width=1200, height=1000)
    event(2, 'published', 410, draw_calls=715)
    event(2, 'preview_complete', 425, status=4)
    event(5, 'ui_draw_begin', 416)
    event(5, 'ui_draw_end', 420)
    event(3, 'compose_begin', 421, cpu_frame=5)
    for stage, at in [('drawable_acquired', 423), ('compose_commit_begin', 430),
                      ('compose_submitted', 435), ('compose_gpu_start', 431),
                      ('compose_gpu_end', 440), ('presented', 450)]:
        event(3, stage, at)
    event(3, 'compose_surface', 422, preview=2)
    event(3, 'compose_complete', 445, status=4)
    event(1, 'first_frame_presented', 450, preview=2, composition=3)
    event(1, 'benchmark_done', 550)
    # Empty-window and unaccepted textures must not win first-presentation selection.
    event(10, 'compose_surface', 140, preview=0)
    event(10, 'presented', 150)
    event(11, 'compose_surface', 160, preview=99)
    event(11, 'presented', 170)
    rows.append({'type': 'end', 'lost_events': 0})
    return rows


class FirstFrameReportTests(unittest.TestCase):
    def test_joins_actual_model_frame_and_gpu_clock_out_of_callback_order(self):
        result = report.analyze(fixture()[::-1])
        self.assertEqual(result['metrics']['load_to_present_ms'], 350)
        self.assertEqual(result['metrics']['load_to_viewport_gpu_end_ms'], 320)
        self.assertEqual(result['metrics']['preview_gpu_ms'], 15)
        self.assertEqual(result['viewport'], {'width': 1200, 'height': 1000})

    def test_rejects_incomplete_failed_or_unjoined_traces(self):
        for stage in ('presented', 'preview_gpu_end', 'compose_gpu_end',
                      'accepted', 'benchmark_done', 'compose_surface'):
            with self.subTest(stage=stage):
                rows = [r for r in fixture() if r.get('stage') != stage]
                with self.assertRaises(ValueError):
                    report.analyze(rows)
        for status in (0, 5):
            rows = fixture()
            next(r for r in rows if r.get('stage') == 'preview_complete')['fields']['status'] = status
            with self.assertRaises(ValueError):
                report.analyze(rows)

    def test_rejects_lost_events_and_dropped_presentations(self):
        rows = fixture()
        rows[-1]['lost_events'] = 1
        with self.assertRaises(ValueError):
            report.analyze(rows)
        rows = fixture()
        next(r for r in rows if r.get('stage') == 'presented' and r['id'] == 3)['stage'] = 'present_dropped'
        with self.assertRaises(ValueError):
            report.analyze(rows)

    def test_later_redraw_cannot_replace_the_first_actual_presentation(self):
        rows = fixture()
        surface = copy.deepcopy(next(r for r in rows if r.get('stage') == 'compose_surface' and r['id'] == 3))
        earlier = copy.deepcopy(next(r for r in rows if r.get('stage') == 'presented' and r['id'] == 3))
        surface['id'] = earlier['id'] = 4
        earlier['at_ns'] -= 1
        rows.extend([surface, earlier])
        with self.assertRaisesRegex(ValueError, 'not the first actual'):
            report.analyze(rows)

    def test_rejects_empty_frames_and_clock_mismatch(self):
        for stage, key in [('request', 'width'), ('published', 'draw_calls')]:
            rows = fixture()
            next(r for r in rows if r.get('stage') == stage)['fields'][key] = 0
            with self.assertRaises(ValueError):
                report.analyze(rows)
        rows = fixture()
        next(r for r in rows if r.get('stage') == 'preview_gpu_end')['at_ns'] = 1
        with self.assertRaisesRegex(ValueError, 'negative interval'):
            report.analyze(rows)

    def test_retained_redraw_must_keep_the_same_rendered_preview(self):
        rows = fixture()
        rows.append({'type': 'event', 'id': 1, 'stage': 'first_frame_redraw',
                     'at_ns': 445_000_000, 'fields': {'composition': 8, 'preview': 2}})
        self.assertEqual(report.analyze(rows)['retained_redraw_requests'], 1)
        rows[-1]['fields']['preview'] = 99
        with self.assertRaisesRegex(ValueError, 'changed model preview'):
            report.analyze(rows)
