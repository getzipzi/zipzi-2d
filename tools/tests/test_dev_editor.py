"""Build/publication controls without compiling graphical dependencies."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'workspace'))
from dev_editor import Editor


class EditorBuild(unittest.TestCase):
    def test_json_artifact_custom_target_and_publication(self):
        with tempfile.TemporaryDirectory(prefix='中文 editor ') as folder:
            editor = Editor(Path(folder))
            for name in ('LICENSE-GPL-3.0', 'LICENSE-MIT', 'LICENSING.md', 'THIRD_PARTY_NOTICES.md'):
                (editor.root / name).write_text('fixture')
            artifact = editor.root / 'custom triple/debug/editor.exe'
            artifact.parent.mkdir(parents=True)
            artifact.write_bytes(b'fixture binary')
            def cargo(command, **kwargs):
                self.assertEqual(command[command.index('--target-dir') + 1], str(Path('custom target').resolve()))
                kwargs['stdout'].write(json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'zipzi-editor', 'kind': ['bin']}, 'executable': str(artifact)}) + '\n')
                return subprocess.CompletedProcess(command, 0)
            with patch.dict(os.environ, ZIPZI_DEV_TARGET_DIR='custom target'), patch('dev_editor.subprocess.run', side_effect=cargo):
                command = editor.build()
            self.assertEqual(command, [str(artifact)])
            self.assertEqual(editor.publish(command), [str(editor.binary)])
            self.assertEqual(editor.binary.read_bytes(), b'fixture binary')
            if sys.platform == 'win32':
                self.assertEqual((editor.binary.parent / 'THIRD_PARTY_NOTICES.md').read_text(), 'fixture')

    def test_build_failure_preserves_published_binary(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            editor.binary.parent.mkdir(parents=True, exist_ok=True)
            editor.binary.write_bytes(b'old')
            with patch('dev_editor.subprocess.run', return_value=subprocess.CompletedProcess([], 17)):
                with self.assertRaises(subprocess.CalledProcessError):
                    editor.reload()
            self.assertEqual(editor.binary.read_bytes(), b'old')

    def test_untracked_process_blocks_reload(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            from dev_process import Identity
            identity = Identity(123, 1., 'fixture')
            with patch('dev_editor.matching', return_value=[identity]):
                with self.assertRaisesRegex(RuntimeError, 'Untracked'):
                    editor.guard([])
                editor.guard([identity])

    def test_windows_exit_refusal_preserves_owner(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            owner = SimpleNamespace(pid=123, process=Mock(return_value=object()))
            def refuse(path, value):
                path.write_text(json.dumps(value))
                path.with_suffix('.reply.json').write_text(json.dumps({
                    'nonce': value['nonce'], 'error': 'Save unsaved documents first'}))
            with patch('dev_editor.sys.platform', 'win32'), patch('dev_editor.save', side_effect=refuse):
                with self.assertRaisesRegex(RuntimeError, 'Save unsaved'):
                    editor.request_stop([owner])
            self.assertIsNotNone(owner.process())
            self.assertEqual(list(editor.state.glob('quit-*')), [])

    def test_windows_missing_exit_reply_times_out_without_stopping_owner(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            owner = SimpleNamespace(pid=123, process=Mock(return_value=object()))
            with patch('dev_editor.sys.platform', 'win32'), patch('dev_editor.time.monotonic', side_effect=[0., 16.]):
                with self.assertRaisesRegex(TimeoutError, 'preserved'):
                    editor.request_stop([owner])
            self.assertIsNotNone(owner.process())
            self.assertEqual(list(editor.state.glob('quit-*')), [])

    def test_windows_exit_acknowledgment_waits_for_process_exit(self):
        with tempfile.TemporaryDirectory() as folder:
            editor = Editor(Path(folder))
            owner = SimpleNamespace(pid=123, process=Mock(side_effect=[object(), object(), None]))
            def acknowledge(path, value):
                path.write_text(json.dumps(value))
                path.with_suffix('.reply.json').write_text(json.dumps({'nonce': value['nonce'], 'error': None}))
            with patch('dev_editor.sys.platform', 'win32'), patch('dev_editor.save', side_effect=acknowledge), patch('dev_editor.time.sleep'):
                editor.request_stop([owner])
            self.assertEqual(owner.process.call_count, 3)
            self.assertEqual(list(editor.state.glob('quit-*')), [])
