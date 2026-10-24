"""Detect ownership escape hatches and view-dependent IO completion."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools/runtime'))
from check_editor_boundary import collect, violations


class EditorBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = collect()

    def test_editor_has_no_formal_write_escape_hatch(self):
        self.assertEqual(violations(self.sources), [])

    def reject(self, name, text):
        sources = dict(self.sources)
        sources[name] += text
        self.assertTrue(violations(sources))

    def test_owned_session_mutable_guard_and_drain_are_detected(self):
        for text in ['struct Escape { sdk: zipzi_sdk::AuthoringSession }',
                     'fn escape(sdk: &mut AuthoringSession) {}',
                     'fn escape(sdk: &mut HostedSession) { sdk.drain_events(); }',
                     'impl DerefMut for SessionRead {}']:
            self.reject('crates/zipzi-editor-core/src/runtime_host.rs', text)

    def test_completion_after_weak_callback_is_rejected(self):
        self.reject('crates/zipzi-editor/src/project.rs', '''
            cx.spawn_in(window, async move |this, cx| {
                let completion = task.run();
                this.update_in(cx, |this, _, _| completion.publish(&runtime));
            });
        ''')

    def test_comments_and_candidate_mutations_are_allowed(self):
        sources = dict(self.sources)
        sources['crates/zipzi-editor-core/src/runtime_host.rs'] += '''
            // AuthoringSession and drain_events() in explanatory text.
            fn candidate(edit: &mut EditSession<'static>) {}
        '''
        self.assertEqual(violations(sources), [])


if __name__ == '__main__':
    unittest.main()
