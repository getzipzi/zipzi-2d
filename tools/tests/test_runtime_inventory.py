"""Check source discovery independently of the old partial binding inventory."""
import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/runtime"))

from runtime_inventory import collect, mask_rust, rust_methods
from runtime_baseline import CASES, summarize


class RuntimeInventoryTests(unittest.TestCase):
    def test_owners_split_impls_free_functions_and_literal_braces(self):
        source = '''
// impl Fake { pub fn ghost() {}}
impl Draft<'_> {
    pub fn first(&mut self, values: Vec<(u32, f32)>) -> Result<(), SdkError> {
        let text = r##"{ impl Fake { pub fn ghost() {} } }"##;
    }
}
/* outer /* pub fn ghost() {} */ comment */
impl crate::Snapshot {
    pub fn read<T>(&self, f: impl FnOnce(&Document) -> T) -> T { f(self.document()) }
    pub(crate) fn internal(&self) {}
}
pub fn helper() -> &'static str { "{" }
'''
        methods = rust_methods(source)
        self.assertEqual([(row["owner"], row["name"]) for row in methods], [
            ("Draft", "first"), ("Snapshot", "read"), ("free", "helper")])
        self.assertIn("Vec<(u32, f32)>", methods[0]["signature"])
        self.assertEqual(len(mask_rust(source)), len(source))

    def test_current_catalog_contains_native_draft_and_session_boundaries(self):
        catalog = collect()
        apis = {row["id"]: row for row in catalog["apis"]}
        self.assertEqual(catalog["scope"], "sdk_source_declarations")
        self.assertEqual(len(apis), len(catalog["apis"]))
        for name in ["sdk::Draft.batch", "sdk::Draft.prepare",
                     "sdk::Commands.edit_motion", "sdk::AuthoringSession.begin_draft",
                     "sdk::AuthoringSession.publish", "sdk::AuthoringSession.undo"]:
            self.assertIn(name, apis)
        self.assertNotIn("sdk::EditSession.edit_motion", apis)
        self.assertEqual(apis["sdk::AuthoringSession.publish"]["location"],
                         "crates/zipzi-sdk/src/authoring/session.rs")

    def test_baseline_rejects_incomplete_duplicate_debug_and_false_validation(self):
        records = [dict(type="metadata", debug_assertions=False, samples=32, warmup=4)]
        for case, count in CASES.items():
            indices = [4] if case == "save_first" else range(count + 4)
            records.extend(dict(type="sample", case=case, index=i, warmup=i < 4,
                                wall_ns=i + 1, peak_bytes=10, retained_bytes=5) for i in indices)
        records.append(dict(type="complete", validated=True))
        self.assertEqual(set(summarize(records)["measurements"]), set(CASES))
        for corrupted in [records[:-1], records[:5] + records[6:], records[:5] + records[4:],
                          [dict(records[0], debug_assertions=True)] + records[1:],
                          records[:-1] + [dict(type="complete", validated=False)]]:
            with self.assertRaises(ValueError):
                summarize(corrupted)


if __name__ == "__main__":
    unittest.main()
