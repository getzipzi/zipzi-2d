import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'windows_gpui_report', Path(__file__).resolve().parents[1] / 'benchmarks/windows-gpui-trace-report.py')
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def fixture():
    records = [{'type': 'metadata', 'samples': 1, 'workloads': [1]}]

    def event(time, id, stage, **fields):
        records.append(dict(type='event', at_ns=time * 1_000_000, id=id, stage=stage, fields=fields))

    event(1, 1, 'benchmark_tick', mode=1, warmup=1, timer_late_ns=0)
    event(2, 10, 'ui_draw_begin')
    event(12, 10, 'ui_draw_end')  # Warmup must not enter distributions.
    event(20, 2, 'benchmark_tick', mode=1, warmup=0, timer_late_ns=1_000_000)
    event(21, 3, 'input_begin', parent=2)
    event(22, 4, 'scene_begin', input=3)
    event(23, 4, 'request')
    event(24, 4, 'published')
    event(25, 5, 'ui_draw_begin')
    event(26, 4, 'compose_image', image=99)
    event(26, 6, 'image_atlas', end_ns=27_000_000, cpu_frame=5, image=99, miss=1, bytes=64)
    event(28, 5, 'ui_draw_end')
    event(29, 5, 'platform_draw_begin')
    event(30, 5, 'platform_draw_end')
    event(31, 5, 'platform_draw_begin')  # Same CPU scene can be presented repeatedly.
    event(33, 5, 'platform_draw_end')
    event(300, 7, 'benchmark_done')
    records.append({'type': 'end', 'lost_events': 0})
    return records


class WindowsGpuiReport(unittest.TestCase):
    def test_input_sync_uses_the_outer_tick_and_includes_effect_flush(self):
        records = fixture()
        for time, id, stage in [(22, 3, 'input_end'), (24, 2, 'input_end')]:
            records.insert(-2, dict(type='event', at_ns=time * 1_000_000,
                                    id=id, stage=stage, fields={}))
        metric = report.diagnose(records)['workloads']['1']['metrics']['input_sync_ms']
        self.assertEqual(metric, report.distribution([4]))

    def test_long_interval_counts_do_not_label_normal_60_hz_jitter_as_stalls(self):
        metric = report.distribution([16.66, 16.70, 19., 33.4, 50.])
        self.assertEqual(metric['over_25_ms'], 2)
        self.assertEqual(metric['p99'], 50.)

    def test_animation_evaluation_joins_to_its_input_and_shared_surface(self):
        records = fixture()
        records[0]['workloads'] = [9]
        records[0]['animation_drag'] = 'number'
        for event in records:
            if event.get('stage') == 'benchmark_tick':
                event['fields']['mode'] = 9
            elif event.get('stage') == 'scene_begin':
                event['stage'] = 'animation_enqueued'
            elif event.get('stage') == 'compose_image':
                event['stage'] = 'paint_shared_surface'
        records = [event for event in records if event.get('stage') != 'request']
        result = report.diagnose(records)['workloads']['9']
        self.assertEqual(result['requests'], 1)
        self.assertEqual(result['inputs_with_image_platform_return'], 1)
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms']['p50'], 10)
        for event in records:
            if event.get('stage') == 'animation_enqueued':
                event['fields']['input'] = 0
        result = report.diagnose(records)['workloads']['9']
        self.assertEqual(result['inputs_with_image_platform_return'], 0)

    def test_publish_to_paint_latency_ignores_later_repaints(self):
        records = fixture()
        # Repaint the same image both within and after the workload's settling
        # window. Neither repaint represents another publication/delivery.
        for start, frame in [(100, 20), (280, 21)]:
            for time, id, stage, fields in [
                (start, frame, 'ui_draw_begin', {}),
                (start + 1, 4, 'compose_image', {'image': 99}),
                (start + 2, frame, 'ui_draw_end', {}),
                (start + 3, frame, 'platform_draw_begin', {}),
                (start + 4, frame, 'platform_draw_end', {}),
            ]:
                records.insert(-2, dict(type='event', at_ns=time * 1_000_000,
                                        id=id, stage=stage, fields=fields))
        result = report.diagnose(records)['workloads']['1']
        self.assertEqual(result['metrics']['worker_publish_to_image_paint_ms'],
                         report.distribution([2]))
        self.assertEqual(result['metrics']['platform_draw_cpu_ms']['n'], 3)
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms']['p50'], 10)
        # If the first painted scene never reaches the platform, its later
        # repaint must still be eligible for the input-to-platform join.
        records = [e for e in records if not (e.get('id') == 5
                   and e.get('stage') in ('platform_draw_begin', 'platform_draw_end'))]
        result = report.diagnose(records)['workloads']['1']
        self.assertEqual(result['metrics']['worker_publish_to_image_paint_ms'],
                         report.distribution([2]))
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms']['p50'], 84)

    def test_publish_to_paint_latency_counts_each_preview_of_one_input(self):
        records = fixture()
        # A single input can request images for multiple viewports.
        for time, id, stage, fields in [
            (22, 8, 'scene_begin', {'input': 3}),
            (23, 8, 'request', {}),
            (24, 8, 'published', {}),
            (27, 8, 'compose_image', {'image': 100}),
        ]:
            records.insert(-2, dict(type='event', at_ns=time * 1_000_000,
                                    id=id, stage=stage, fields=fields))
        result = report.diagnose(records)['workloads']['1']
        self.assertEqual(result['metrics']['worker_publish_to_image_paint_ms'],
                         report.distribution([2, 3]))

    def test_warmup_parent_join_and_repeated_platform_calls(self):
        result = report.diagnose(fixture())['workloads']['1']
        self.assertEqual(result['metrics']['ui_draw_ms']['p50'], 3)
        self.assertEqual(result['metrics']['platform_draw_cpu_ms']['n'], 2)
        self.assertEqual(result['metrics']['platform_draw_cpu_ms']['p50'], 1.5)
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms']['p50'], 10)
        self.assertEqual(result['inputs_without_image_platform_return'], 0)
        self.assertEqual(result['atlas_uploaded_bytes'], 64)

    def test_missing_frames_are_reported_without_inventing_display_latency(self):
        records = [e for e in fixture() if e.get('stage') != 'compose_image']
        result = report.diagnose(records)['workloads']['1']
        self.assertEqual(result['inputs_without_image_platform_return'], 1)
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms'], {'n': 0})

    def test_shared_texture_paint_joins_without_readback_or_atlas_upload(self):
        records = [e for e in fixture() if e.get('stage') not in ('readback', 'image_atlas')]
        for event in records:
            if event.get('stage') == 'compose_image':
                event['stage'] = 'paint_shared_surface'
                event['fields'] = {}
        result = report.diagnose(records)['workloads']['1']
        self.assertEqual(result['inputs_without_image_platform_return'], 0)
        self.assertEqual(result['atlas_uploaded_bytes'], 0)
        self.assertEqual(result['metrics']['input_to_first_image_platform_return_ms']['p50'], 10)

    def test_incomplete_lost_or_failed_run_is_rejected(self):
        with self.assertRaises(ValueError):
            report.diagnose(fixture()[:-1])
        records = fixture()
        records[-1]['lost_events'] = 1
        with self.assertRaises(ValueError):
            report.diagnose(records)
        records = fixture()
        records[-2]['stage'] = 'benchmark_failed'
        with self.assertRaises(ValueError):
            report.diagnose(records)


if __name__ == '__main__':
    unittest.main()
