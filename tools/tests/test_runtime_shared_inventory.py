"""The native inventory must not inspect Python client implementation sources."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('shared_runtime_inventory', ROOT / 'tools/runtime/runtime_inventory.py')
inventory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(inventory)

class SharedInventory(unittest.TestCase):
    def test_native_inventory_is_independent_of_python_sources(self):
        read = Path.read_text
        def guarded(path, *args, **kwargs):
            if '/crates/zipzi-python/' in str(path):
                raise AssertionError('Retired client source read')
            return read(path, *args, **kwargs)
        with patch.object(Path, 'read_text', guarded):
            catalog = inventory.collect()
        rows = {row['id']: row for row in catalog['apis']}
        self.assertIn('sdk::Draft.batch', rows)
        self.assertTrue(all(row['location'].startswith('crates/zipzi-sdk/src/') for row in rows.values()))
