"""Negative controls for acceptance gates; run with unittest discovery."""
import contextlib
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools/validation'))

import compare_sdk_image as images
import compare_wgpu_blends as blends
import validate_official_core as core


class ConformanceGateTests(unittest.TestCase):
    def check_gate(self, mutate=None, empty_samples=False, missing_fixture=False):
        expected = dict(id='mesh', runtime_id='mesh', texture_slot=0, draw_order=0,
                        render_order=0, double_sided=True, inverted_mask=False,
                        blend_mode=0, indices=[0, 1, 2], masks=[], opacity=1,
                        positions=[dict(x=0, y=0), dict(x=100, y=0), dict(x=0, y=100)],
                        uvs=[dict(x=0, y=0), dict(x=1, y=0), dict(x=0, y=1)],
                        multiply_color=[1, 1, 1, 1], screen_color=[0, 0, 0, 1])
        actual = copy.deepcopy(expected)
        actual['mask_indices'] = []
        actual['positions'] = [[p['x'] / core.CONFORMANCE_PIXELS_PER_UNIT,
                                p['y'] / core.CONFORMANCE_PIXELS_PER_UNIT]
                               for p in expected['positions']]
        actual['uvs'] = [[p['x'], p['y']] for p in expected['uvs']]
        if mutate:
            mutate(actual)
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)
            case = output / 'fixtures' / 'case'
            case.mkdir(parents=True)
            (case / 'model.moc3').write_bytes(b'MOC3')
            if not missing_fixture:
                (case / 'samples.json').write_text(json.dumps([] if empty_samples else [dict(parameters=[0], drawables=[expected])]))
            # A stale passing report must not survive any failed attempt.
            (output / 'report.json').write_text('{"status":"passed"}')
            runtime = dict(core_version=1, samples=[[actual]])
            with patch('sys.argv', ['validator', '--output-dir', folder]), patch.object(core, 'ensure_probes', return_value=('official', 'purism')), patch.object(core, 'run', return_value=json.dumps(runtime)), contextlib.redirect_stdout(io.StringIO()):
                if mutate or empty_samples or missing_fixture:
                    with self.assertRaises(RuntimeError):
                        core.main()
                    self.assertEqual(json.loads((output / 'report.json').read_text())['status'], 'failed')
                else:
                    self.assertEqual(core.main(), 0)

    def test_valid_data_passes(self):
        self.check_gate()

    def test_empty_arrays_rejected(self):
        for key in ('positions', 'uvs', 'multiply_color', 'screen_color'):
            with self.subTest(key=key):
                self.check_gate(lambda d: d.__setitem__(key, []))

    def test_truncated_nonempty_arrays_rejected(self):
        self.check_gate(lambda d: (d.__setitem__('positions', d['positions'][:1]), d.__setitem__('uvs', d['uvs'][:1])))

    def test_nonfinite_rejected(self):
        for value in (float('nan'), float('inf'), -float('inf')):
            for key in ('opacity', 'draw_order'):
                with self.subTest(value=value, key=key):
                    self.check_gate(lambda d: d.__setitem__(key, value))
            self.check_gate(lambda d: d['positions'][0].__setitem__(0, value))

    def test_empty_samples_rejected(self):
        self.check_gate(empty_samples=True)

    def test_missing_fixture_rejected(self):
        self.check_gate(missing_fixture=True)

    def test_wrong_geometry_rejected(self):
        self.check_gate(lambda d: d['positions'][0].__setitem__(1, 0.1))


class BlendReferenceTests(unittest.TestCase):
    def test_blend_matrix_rejects_one_wrong_tile(self):
        width, height, original = images.read_png(blends.REFERENCE)
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)
            changed = bytearray(original)
            changed[:4] = b'\xff\xff\xff\xff'
            actual = output / 'changed.png'
            images.write_png(actual, width, height, changed)
            report = blends.compare(actual, output)
            self.assertEqual(report['status'], 'failed')
            self.assertFalse(report['cases'][0]['passed'])


if __name__ == '__main__':
    unittest.main()
