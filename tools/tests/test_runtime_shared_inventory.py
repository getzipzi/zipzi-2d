"""The Python2 workflow must not inspect the retired client implementation."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('shared_runtime_inventory', ROOT / 'tools/runtime/runtime_inventory.py')
inventory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(inventory)

class SharedInventory(unittest.TestCase):
    def test_shared_refresh_keeps_historical_rows_without_reading_retired_sources(self):
        read = Path.read_text
        def guarded(path, *args, **kwargs):
            if '/crates/zipzi-python/' in str(path):
                raise AssertionError('Retired client source read')
            return read(path, *args, **kwargs)
        with patch.object(Path, 'read_text', guarded):
            catalog = inventory.collect(shared_only=True)
        rows = {row['id']: row for row in catalog['apis']}
        operation = rows['sdk::SceneOperation.apply']
        self.assertEqual(operation['classification'], 'connected')
        self.assertEqual(operation['source_ref'], 'edit + candidate sequence')
        self.assertEqual(rows['sdk::SceneSummary.response_bytes']['classification'], 'host-only')
        self.assertTrue(any(v['name']=='SceneOperation' for v in catalog['domain_intents']))
