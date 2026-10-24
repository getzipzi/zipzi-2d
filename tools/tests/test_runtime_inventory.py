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
impl EditSession<'_> {
    pub fn first(&mut self, values: Vec<(u32, f32)>) -> Result<(), SdkError> {
        let text = r##"{ impl Fake { pub fn ghost() {} } }"##;
    }
}
/* outer /* pub fn ghost() {} */ comment */
impl crate::AuthoringSnapshot {
    pub fn read<T>(&self, f: impl FnOnce(&Document) -> T) -> T { f(self.document()) }
    pub(crate) fn internal(&self) {}
}
pub fn helper() -> &'static str { "{" }
'''
        methods = rust_methods(source)
        self.assertEqual([(row["owner"], row["name"]) for row in methods], [
            ("EditSession", "first"), ("AuthoringSnapshot", "read"), ("free", "helper")])
        self.assertIn("Vec<(u32, f32)>", methods[0]["signature"])
        self.assertEqual(len(mask_rust(source)), len(source))

    def test_real_catalog_includes_split_modules_facade_reexports_and_intents(self):
        catalog = collect()
        apis = {row["id"]: row for row in catalog["apis"]}
        for name in ["sdk::EditSession.edit_motion", "sdk::AuthoringSession.plan_document_edit",
                     "sdk::MotionPreview.seek", "python::Session.plan_binding_authoring",
                     "python::Edit.__exit__", "python::_native.capabilities", "python::SdkFailure",
                     "python::ObjectHandle.id"]:
            self.assertIn(name, apis)
        self.assertNotIn("sdk::Occupied.new", apis)
        self.assertEqual(apis["sdk::ImageLoadToken.cancel"]["classification"], "connected")
        self.assertEqual(apis["sdk::ObjectHandle.id"]["classification"], "local-value")
        self.assertEqual(apis["python::_session.open_project"]["classification"], "connected")
        self.assertIn("replacement", apis["sdk::AuthoringSession.new_project"]["effect"])
        self.assertTrue(any(row["name"] == "MotionEdit" for row in catalog["domain_intents"]))
        self.assertTrue(any(row["name"] == "NativeBindingPlan" for row in catalog["native_types"]))
        for row in catalog["apis"]:
            self.assertIn(row["classification"], {"connected", "host-only", "local-helper", "local-value"})
            self.assertTrue(row["acceptance"])
            self.assertEqual(row["implementation"], "not-connected")

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
