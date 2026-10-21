import copy
import importlib.util
import json
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("property_chain_benchmark", Path(__file__).with_name("property-chain-benchmark.py"))
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


class PropertyChainComparisonTests(unittest.TestCase):
    def test_comparison_rejects_changed_workload_source_or_model(self):
        spec = importlib.util.spec_from_file_location("chain_compare", Path(__file__).with_name("property-chain-compare.py"))
        compare = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(compare)
        before = {
            "arguments": dict(model="mao", samples=60, warmup=10, updates=12, filter="all", reader_policy="unique", rounds=3),
            "sources": {"kasane-editor/crates/editor-core/examples/property_chain_benchmark.rs": "same"},
            "model_files": {"mao.moc3": "same"},
            "rustc": "rustc test compiler",
            "build_environment": {"RUSTFLAGS": "-C opt-level=3"},
        }
        compare.compatible(before, copy.deepcopy(before))
        relocated = copy.deepcopy(before)
        relocated["build_environment"]["CARGO_TARGET_DIR"] = "/tmp/isolated-build"
        compare.compatible(before, relocated)
        for section, key, value in [
            ("arguments", "reader_policy", "retained"),
            ("arguments", "updates", 1),
            ("sources", "kasane-editor/crates/editor-core/examples/property_chain_benchmark.rs", "different"),
            ("model_files", "mao.moc3", "different"),
        ]:
            after = copy.deepcopy(before)
            after[section][key] = value
            with self.assertRaises(ValueError):
                compare.compatible(before, after)
        for field, value in [
            ("rustc", "different compiler"),
            ("build_environment", {"RUSTFLAGS": "-C opt-level=0"}),
            ("build_environment", {"CARGO_PROFILE_RELEASE_LTO": "true"}),
        ]:
            after = copy.deepcopy(before)
            after[field] = value
            with self.assertRaises(ValueError):
                compare.compatible(before, after)


if __name__ == "__main__":
    unittest.main()
