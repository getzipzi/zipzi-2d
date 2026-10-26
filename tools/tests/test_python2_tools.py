"""Independent package build boundaries and P0 evidence validation."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import sys
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/zipzi-python2"
sys.path.insert(0, str(PACKAGE / "tools"))
SPEC = importlib.util.spec_from_file_location("python2_baseline", PACKAGE / "tools/baseline.py")
baseline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(baseline)

class Python2ToolsTests(unittest.TestCase):
    def record(self):
        return dict(validated=True, extension_sha256="same", client_sha256="client", measurement_sha256="tool", cases={case:
            dict(wall_ns=list(range(1, baseline.SAMPLES + 1)), probe=dict(copied_bytes=None)) for case in baseline.CASES})

    def test_measurement_rejects_missing_cases_samples_validation_and_binary_drift(self):
        run = self.record()
        summary = baseline.summarize([run, deepcopy(run)])
        self.assertEqual(summary["small_query"]["p50_ns"], 16.5)
        self.assertIsNone(summary["large_array"]["probes"][0]["copied_bytes"])
        for corrupt in (
            lambda r: r.update(validated=False),
            lambda r: r.update(extension_sha256="other"),
            lambda r: r.update(client_sha256="other"),
            lambda r: r.update(measurement_sha256="other"),
            lambda r: r["cases"].pop("batch_100"),
            lambda r: r["cases"]["large_array"]["wall_ns"].pop(),
            lambda r: r["cases"]["single_edit"]["wall_ns"].__setitem__(0, 0),
        ):
            broken = deepcopy(run)
            corrupt(broken)
            with self.assertRaises(ValueError):
                baseline.summarize([run, broken])

    def test_client_and_native_are_distinct_distributions_with_matching_versions(self):
        client = tomllib.loads((PACKAGE / "client/pyproject.toml").read_text())
        native = tomllib.loads((PACKAGE / "pyproject.toml").read_text())
        cargo = tomllib.loads((PACKAGE / "Cargo.toml").read_text())
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(cargo["package"]["name"], "zipzi-python2")
        self.assertEqual(client["project"]["name"], "zipzi")
        self.assertEqual(native["project"]["name"], "zipzi-native")
        version = workspace["workspace"]["package"]["version"]
        sys.path.insert(0, str(ROOT / "tools/packaging"))
        from release import versions
        python_version = versions(version)["python_version"]
        self.assertEqual(client["project"]["version"], python_version)
        self.assertIn(f"zipzi=={python_version}", native["project"]["dependencies"])
        self.assertNotIn("dependencies", client["project"])
        self.assertEqual(native["tool"]["maturin"]["module-name"], "zipzi_native._native")
        self.assertNotIn("zipzi-python", cargo["dependencies"])
        for directory in (PACKAGE, PACKAGE / "client"):
            for name in ("LICENSE-MIT", "THIRD_PARTY_NOTICES.md"):
                self.assertEqual((directory / name).read_bytes(), (ROOT / name).read_bytes())


class Python2ProbeTests(unittest.TestCase):
    def probe(self, mode="local"):
        return dict(rust=dict(enabled=True, rust_live_bytes=110, rust_start_bytes=100,
            rust_peak_bytes=160, rust_allocations=10, rust_allocated_bytes=100,
            json_encodes=4 if mode == "connected" else 0,
            json_decodes=4 if mode == "connected" else 0,
            json_value_builds=0, json_admission_encodes=1,
            materialized_input_bytes=16, copied_payload_bytes=64),
            image_memory_after=dict(prepared_images=0, plans=0, preparing_bytes=0),
            calls={"native.apply": 1} if mode == "local" else {"python_json.dumps": 8})

    def test_probe_requires_actual_bounded_counts_and_distinguishes_wire_from_admission(self):
        baseline.validate_probe("local", self.probe())
        baseline.validate_probe("connected", self.probe("connected"))
        for corrupt in (
            lambda p: p["rust"].update(enabled=False),
            lambda p: p["rust"].update(copied_payload_bytes=None),
            lambda p: p["rust"].update(rust_peak_bytes=0),
            lambda p: p["rust"].update(json_decodes=1),
            lambda p: p["image_memory_after"].update(plans=1),
        ):
            probe = self.probe()
            corrupt(probe)
            with self.assertRaises(ValueError):
                baseline.validate_probe("local", probe)
        with self.assertRaises(ValueError):
            baseline.validate_probe("connected", self.probe())

    def report(self):
        return dict(schema=2, measurement_sha256="measurement", platform="test", python="3.14",
            samples=32, warmup=4,
            thresholds=dict(frozen=True, p50_ratio=1.5, p95_ratio=2., max_process_p50_range=.5,
                wire_poll_codec_allowance=3, max_rust_peak_growth_ratio=1.5),
            measurements={mode: {"small_query": dict(p50_ns=100, p95_ns=150, relative_p50_range=.1,
                probes=[self.probe(mode)])} for mode in ("local", "connected")})

    def test_frozen_gate_rejects_latency_copy_heap_codecs_and_noise_regressions(self):
        reference = self.report()
        self.assertEqual(baseline.compare(self.report(), reference), [])
        for metric, mutate in (
            ("p50", lambda c: c.update(p50_ns=151)),
            ("p95", lambda c: c.update(p95_ns=301)),
            ("noise", lambda c: c.update(relative_p50_range=.51)),
            ("copied_payload_bytes", lambda c: c["probes"][0]["rust"].update(copied_payload_bytes=65)),
            ("json_encodes", lambda c: c["probes"][0]["rust"].update(json_encodes=1)),
            ("rust_peak", lambda c: c["probes"][0]["rust"].update(rust_peak_bytes=191)),
        ):
            changed = self.report()
            mutate(changed["measurements"]["local"]["small_query"])
            self.assertIn(f"local.small_query.{metric}", baseline.compare(changed, reference))
        another = self.report()
        second = self.probe()
        second["calls"]["native.apply"] = 2
        another["measurements"]["local"]["small_query"]["probes"].append(second)
        self.assertIn("local.small_query.native.apply", baseline.compare(another, reference))
        incompatible = self.report()
        incompatible["measurement_sha256"] = "changed workload"
        with self.assertRaises(ValueError):
            baseline.compare(incompatible, reference)
        reference["thresholds"]["frozen"] = False
        with self.assertRaises(ValueError):
            baseline.compare(self.report(), reference)

if __name__ == "__main__":
    unittest.main()
