"""Contract sharding, integrity and replay boundary checks."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'crates/zipzi-python2/tools'))
from codegen import schema
from codegen.model import TypeGraph


class Schema(unittest.TestCase):
    def setUp(self):
        self.logical = schema.read(ROOT / 'crates/zipzi-python2/schema/authoring.json')
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for name, text in schema.serialize(self.logical).items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding='utf-8')
        self.index = self.root / 'schema/authoring.json'

    def write_index(self, data):
        self.index.write_text(json.dumps(data), encoding='utf-8')

    def test_replay_covers_every_definition_and_digest(self):
        self.assertEqual(schema.read(self.index), self.logical)
        self.assertEqual(schema.digest(schema.read(self.index)), schema.digest(self.logical))
        self.assertEqual(len(TypeGraph.from_contract(self.logical).definitions), 327)

    def test_modified_shard_and_wrong_logical_digest_are_rejected(self):
        index = json.loads(self.index.read_text())
        shard = self.index.parent / index['shards'][0]['path']
        shard.write_text(shard.read_text()+' ')
        with self.assertRaisesRegex(ValueError, 'shard digest mismatch'):
            schema.read(self.index)
        index['shards'][0]['sha256'] = hashlib.sha256(shard.read_bytes()).hexdigest()
        index['contract_sha256'] = '0'*64
        self.write_index(index)
        with self.assertRaisesRegex(ValueError, 'Logical contract digest'):
            schema.read(self.index)

    def test_traversal_symlink_and_duplicate_paths_are_rejected(self):
        original = json.loads(self.index.read_text())
        for relative in ('../outside.json', '/tmp/outside.json', 'types/../outside.json',
                         'types/..\\outside.json', 'types/mesh.json:stream.json'):
            index = json.loads(json.dumps(original))
            index['shards'][0]['path'] = relative
            self.write_index(index)
            with self.subTest(path=relative), self.assertRaisesRegex(ValueError, 'shard path'):
                schema.read(self.index)
        index = json.loads(json.dumps(original))
        index['shards'].append(index['shards'][0])
        self.write_index(index)
        with self.assertRaisesRegex(ValueError, 'shard path'):
            schema.read(self.index)
        self.write_index(original)
        shard = self.index.parent / original['shards'][0]['path']
        shard.unlink()
        shard.symlink_to(self.index)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            schema.read(self.index)

    def test_duplicate_json_keys_and_unknown_references_are_rejected(self):
        self.index.write_text('{"format_version":2,"format_version":2}', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'Duplicate JSON key'):
            schema.read(self.index)
        logical = json.loads(json.dumps(self.logical))
        logical['root']['roots'].append({'kind':'ref','name':'unknown'})
        with self.assertRaisesRegex(ValueError, 'Unresolved type reference'):
            TypeGraph.from_contract(logical)


if __name__ == '__main__':
    unittest.main()
