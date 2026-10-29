"""Release promotion must retain tested bytes, commit identity and trusted provenance."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('reuse_release',
    Path(__file__).resolve().parents[1] / 'packaging/reuse_release.py')
reuse = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reuse)


class ReusableBuilds(unittest.TestCase):
    def run_record(self, **changes):
        return dict(dict(id=123, head_sha='abc', head_branch='main',
            head_repository={'full_name': 'getzipzi/zipzi-2d'}, event='push',
            status='completed', conclusion='success'), **changes)

    def artifacts(self):
        return {'artifacts': [dict(name=name, expired=False) for name in sorted(reuse.ARTIFACTS)]}

    def test_only_successful_exact_commit_builds_from_default_branch_are_reused(self):
        for event in ('push', 'workflow_dispatch'):
            with self.subTest(event=event):
                def api(path):
                    if '/artifacts?' in path:
                        return self.artifacts()
                    self.assertIn('head_sha=abc', path)
                    return {'workflow_runs': [self.run_record(event=event)]}
                self.assertEqual(reuse.find_build('getzipzi/zipzi-2d', 'abc', 'main', api), '123')

    def test_forks_other_revisions_prs_tags_and_incomplete_runs_are_rejected(self):
        cases = [dict(head_sha='other'), dict(head_branch='feature'),
            dict(head_repository={'full_name': 'someone/fork'}), dict(head_repository=None),
            dict(event='pull_request'), dict(head_branch='v0.1.0-alpha.2'),
            dict(conclusion='failure'), dict(status='in_progress'), dict(id='123')]
        for changes in cases:
            with self.subTest(changes=changes):
                def api(path):
                    self.assertNotIn('/artifacts?', path)
                    return {'workflow_runs': [self.run_record(**changes)]}
                self.assertEqual(reuse.find_build('getzipzi/zipzi-2d', 'abc', 'main', api), '')

    def test_missing_or_expired_artifacts_fall_back_to_an_older_valid_build(self):
        for missing in (True, False):
            with self.subTest(missing=missing):
                def api(path):
                    if '/runs?' in path:
                        return {'workflow_runs': [self.run_record(id=124), self.run_record()]}
                    artifacts = self.artifacts()
                    if '/124/' in path:
                        if missing:
                            artifacts['artifacts'].pop()
                        else:
                            artifacts['artifacts'][0]['expired'] = True
                    return artifacts
                self.assertEqual(reuse.find_build('getzipzi/zipzi-2d', 'abc', 'main', api), '123')

    def test_api_failure_requests_a_normal_build(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / 'github-output'
            arguments = ['reuse_release.py', 'find', '--repository', 'getzipzi/zipzi-2d',
                '--branch', 'main', '--revision', 'abc', '--github-output', str(output)]
            with patch.object(sys, 'argv', arguments), patch.object(reuse, 'find_build',
                    side_effect=subprocess.CalledProcessError(1, ['gh', 'api'])):
                reuse.main()
            self.assertEqual(output.read_text(), 'run_id=\n')


class BuildManifest(unittest.TestCase):
    def fixture(self, root):
        assets, distributions = root / 'release', root / 'dist'
        assets.mkdir()
        distributions.mkdir()
        for name in ('zipzi-0.1.0a2-py3-none-any.whl', 'zipzi_native-0.1.0a2.tar.gz'):
            (assets / name).write_bytes(name.encode())
            (distributions / name).write_bytes(name.encode())
        (assets / 'Editor.zip').write_bytes(b'editor')
        (assets / 'SHA256SUMS').write_text('fixture checksums')
        manifest = root / 'metadata/build.json'
        reuse.write_manifest(manifest, assets, 'abc', 'v0.1.0-alpha.2')
        return manifest, assets, distributions

    def test_matching_identity_and_publication_bytes_pass(self):
        with tempfile.TemporaryDirectory() as folder:
            manifest, assets, dist = self.fixture(Path(folder))
            reuse.verify_manifest(manifest, assets, dist, 'abc', 'v0.1.0-alpha.2')

    def test_changed_missing_extra_or_wrong_identity_payloads_are_rejected(self):
        cases = ('revision', 'tag', 'app', 'checksum', 'extra', 'missing', 'python', 'python-extra')
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as folder:
                manifest, assets, dist = self.fixture(Path(folder))
                revision, tag = 'abc', 'v0.1.0-alpha.2'
                if case == 'revision': revision = 'def'
                elif case == 'tag': tag = 'v0.1.0-alpha.3'
                elif case == 'app': (assets / 'Editor.zip').write_bytes(b'changed')
                elif case == 'checksum': (assets / 'SHA256SUMS').write_text('changed')
                elif case == 'extra': (assets / 'unexpected.zip').touch()
                elif case == 'missing': (assets / 'Editor.zip').unlink()
                elif case == 'python': next(dist.iterdir()).write_bytes(b'changed')
                else: (dist / 'unexpected.whl').touch()
                with self.assertRaises(ValueError):
                    reuse.verify_manifest(manifest, assets, dist, revision, tag)


if __name__ == '__main__':
    unittest.main()
