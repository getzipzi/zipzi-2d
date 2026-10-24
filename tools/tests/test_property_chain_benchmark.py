import copy
import importlib.util
import json
from pathlib import Path
import unittest
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/benchmarks"))
from unittest import mock
import tempfile
import subprocess

SPEC = importlib.util.spec_from_file_location("property_chain_benchmark", (Path(__file__).resolve().parents[2] / "tools").joinpath("benchmarks/property-chain-benchmark.py"))
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)


def records(instrumented=False):
    rows = [{"type":"metadata", "instrumented":instrumented, "samples":2, "warmup":1, "updates":2}]
    for sample in range(2):
        for phase, times in [("commit", [10,1000]), ("after_commit", [1000,10])]:
            wall = times[sample]
            stages = {
                "benchmark.operation":{"calls":1, "inclusive_ns":wall-1, "self_ns":1},
                "child":{"calls":2, "inclusive_ns":wall-2, "self_ns":wall-2},
            } if instrumented else {}
            rows.append({"type":"sample", "case":"mesh_commit", "phase":phase, "sample":sample, "update":0, "wall_ns":wall, "stages":stages})
    rows.append({"type":"complete", "validated":True})
    return rows


class PropertyChainReportTests(unittest.TestCase):
    def test_uses_cargo_artifact_path_for_configured_build_target(self):
        binary = "/tmp/custom-target/aarch64-apple-darwin/release/examples/property_chain_benchmark"
        artifact = {
            "reason": "compiler-artifact",
            "target": {"name": "property_chain_benchmark", "kind": ["example"]},
            "executable": binary,
        }
        messages = [
            {"reason": "compiler-artifact", "target": {"name": "other", "kind": ["example"]}, "executable": "/tmp/other"},
            artifact,
            {"reason": "build-finished", "success": True},
        ]
        self.assertEqual(BENCH.probe_executable("\n".join(map(json.dumps, messages))), Path(binary))
        with self.assertRaises(RuntimeError):
            BENCH.probe_executable(json.dumps(messages[-1]))
        with self.assertRaises(RuntimeError):
            BENCH.probe_executable("\n".join(map(json.dumps, [artifact, {**artifact, "executable": "/tmp/stale"}])))

    def test_source_archive_and_provenance_use_the_workspace_root(self):
        self.assertEqual(BENCH.ROOT, Path(__file__).resolve().parents[2])
        sources = BENCH.source_manifest()
        self.assertIn('Cargo.lock', sources)
        self.assertIn('tools/benchmarks/property-chain-benchmark.py', sources)
        self.assertIn('crates/zipzi-sdk/Cargo.toml', sources)
        self.assertNotIn('apps/editor/Cargo.lock', sources)
        self.assertFalse(any('models/local/' in path or path.startswith('../') for path in sources))
        with mock.patch.object(BENCH, 'source_manifest', return_value={'Cargo.lock':'hash'}), \
             mock.patch.object(BENCH, 'repository', return_value={'head':'single-head','status':'dirty'}), \
             mock.patch.object(BENCH, 'command', return_value='fixture'), \
             mock.patch.object(BENCH.platform, 'system', return_value='Fixture'), \
             tempfile.TemporaryDirectory() as tmp:
            model = Path(tmp)/'model.json'
            model.write_text('{}')
            output = Path(tmp)/'output'
            with mock.patch('sys.argv', ['benchmark',str(model),str(output)]), \
                 mock.patch.object(BENCH.subprocess, 'run', side_effect=subprocess.CalledProcessError(7,'cargo')):
                with self.assertRaises(subprocess.CalledProcessError):
                    BENCH.main()
            import tarfile
            with tarfile.open(output/'sources.tar.gz') as archive:
                self.assertEqual(archive.getnames(), ['Cargo.lock'])
            self.assertTrue((output/'repository.diff').exists())
            self.assertFalse((output/'editor.diff').exists())
            self.assertFalse((output/'sdk.diff').exists())

    def test_refuses_partial_or_duplicate_runs(self):
        with self.assertRaises(ValueError):
            BENCH.validate(records()[:-1])
        rows = records()
        rows.insert(1, copy.deepcopy(rows[1]))
        with self.assertRaises(ValueError):
            BENCH.validate(rows)
        rows = records()
        del rows[1]
        with self.assertRaises(ValueError):
            BENCH.validate(rows)

    def test_nested_accounting_reconciles_without_adding_inclusive_times(self):
        result = BENCH.summarize(records(True))["measurements"]["mesh_commit/commit"]
        self.assertEqual(result["stages"]["child"]["calls_per_operation"], 2)
        rows = records(True)
        rows[1]["stages"]["child"]["self_ns"] += 1
        with self.assertRaises(ValueError):
            BENCH.validate(rows)

    def test_combined_latency_uses_paired_events_not_sum_of_percentiles(self):
        result = BENCH.summarize(records())["measurements"]
        self.assertEqual(result["mesh_commit/commit_and_preview"]["p95_us"], 1.01)
        self.assertEqual(result["mesh_commit/commit"]["p95_us"], 1)
        self.assertEqual(result["mesh_commit/after_commit"]["p95_us"], 1)

    def test_refuses_wrong_feature_and_missing_phases(self):
        rows = records(True)
        rows[0]["instrumented"] = False
        with self.assertRaises(ValueError):
            BENCH.validate(rows)
        rows = records()
        rows = [row for row in rows if row.get("phase") != "after_commit"]
        with self.assertRaises(ValueError):
            BENCH.validate(rows)


if __name__ == "__main__":
    unittest.main()
