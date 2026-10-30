"""Normalization controls independent of an imported SDK or native Runtime."""
import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('python2_normalize', ROOT / 'crates/zipzi-python2/tools/codegen/normalize.py')
normal = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(normal)
sys.path.insert(0, str(ROOT / 'crates/zipzi-python2/tools'))
from codegen.model import TypeGraph as CheckedTypeGraph, Projection, validate_projections, DocEntry
from codegen import projections
from codegen.dependencies import domain_dependencies, strong_components


def record(name, fields=()):
    return {'kind': 'record', 'name': name, 'fields': list(fields)}


def field(name, shape):
    return {'name': name, 'shape': shape}


class TypeGraph(unittest.TestCase):
    def test_unique_definitions_resolve_opaque_and_preserve_replay(self):
        leaf = record('crate::Leaf', [field('n', {'kind': 'u32'})])
        value = {'roots': [record('crate::Parent', [field('leaf', leaf)]), leaf,
                           {'kind': 'opaque', 'name': 'crate::Leaf'}]}
        root, types = normal.normalize(value, set())
        self.assertEqual(list(types), ['crate::Leaf', 'crate::Parent'])
        self.assertEqual(types['crate::Parent']['fields'][0]['shape'], {'kind': 'ref', 'name': 'crate::Leaf'})
        self.assertEqual(types['crate::Leaf']['kind'], 'record')
        self.assertEqual(normal.expand(root, types), value)

    def test_conflicts_missing_references_and_external_types(self):
        with self.assertRaisesRegex(ValueError, 'Conflicting complete'):
            normal.normalize([record('crate::A'), record('crate::A', [field('x', {'kind': 'u32'})])], set())
        with self.assertRaisesRegex(ValueError, 'Unresolved opaque'):
            normal.normalize([{'kind': 'opaque', 'name': 'crate::Missing'}], set())
        with self.assertRaisesRegex(ValueError, 'Unresolved type reference'):
            normal.normalize([{'kind': 'ref', 'name': 'crate::Missing'}], set())
        root, types = normal.normalize([{'kind': 'opaque', 'name': 'crate::External'}], {'crate::External'})
        self.assertEqual(normal.expand(root, types), [{'kind': 'opaque', 'name': 'crate::External'}])

    def test_cycles_are_rejected_and_generic_ids_are_distinct(self):
        with self.assertRaisesRegex(ValueError, 'Cyclic type definition'):
            normal.normalize(record('crate::A', [field('next', {'kind': 'ref', 'name': 'crate::A'})]), set())
        first, second = record('crate::Box<crate::A>'), record('crate::Box<crate::B>')
        _, types = normal.normalize([second, first], set())
        self.assertEqual(list(types), ['crate::Box<crate::A>', 'crate::Box<crate::B>'])

    def test_committed_export_normalizes_without_changing_legacy_render_inputs(self):
        contract = json.loads((ROOT / 'crates/zipzi-python2/schema/authoring.json').read_text())
        from importlib.util import spec_from_file_location, module_from_spec
        spec = spec_from_file_location('python2_projections', ROOT / 'crates/zipzi-python2/tools/codegen/projections.py')
        projections = module_from_spec(spec)
        spec.loader.exec_module(projections)
        root, types = normal.normalize(contract, projections.EXTERNAL_TYPES)
        self.assertGreater(len(types), 300)
        self.assertEqual(normal.expand(root, types), contract)
        graph = CheckedTypeGraph.from_contract(contract)
        self.assertEqual(len(graph.domains), 311)
        self.assertEqual(graph.domains['zipzi_contract::document::DeletionRequest'], 'deletion')
        self.assertEqual(graph.domains['zipzi_contract::model::Vec2'], 'common')
        self.assertEqual(graph.definition('zipzi_contract::document::DeletionRequest')['kind'], 'serde_record')

    def test_projection_coverage_and_conflicting_ownership_are_checked(self):
        with self.assertRaisesRegex(ValueError, 'Missing domain projection'):
            CheckedTypeGraph.from_contract(record('crate::Unreviewed'))
        with patch.object(projections, 'DOMAIN_MEMBERS', {'crate': {'mesh': 'Shared', 'scene': 'Shared'}}):
            with self.assertRaisesRegex(ValueError, 'Conflicting domain projection'):
                projections._build_owners()

    def test_special_projections_and_public_aliases_are_validated(self):
        canonical = Projection('crate::A','canonical','mesh','values.mesh','zipzi.mesh.A',('zipzi.A',))
        special = Projection('crate::A','different','mesh','values.mesh','zipzi.mesh.B')
        with self.assertRaisesRegex(ValueError, 'requires a reason'):
            validate_projections((canonical,special))
        special = Projection('crate::A','different','mesh','values.mesh','zipzi.mesh.B',reason='Different constructor representation')
        validate_projections((canonical,special))
        with self.assertRaisesRegex(ValueError,'Duplicate projection'):
            validate_projections((canonical,canonical))
        other = Projection('crate::B','canonical','mesh','values.mesh','zipzi.A')
        with self.assertRaisesRegex(ValueError,'Conflicting public projection'):
            validate_projections((canonical,other))

    def test_doc_entries_keep_field_and_inherited_semantics_separate(self):
        row = {'kind':'type','documentation':'Meaning.\n\nFields:\nx: Useful value.',
               'fields':[('x','Useful value.')], 'children':['zipzi.A.summary']}
        entry = DocEntry.from_row('zipzi.A',row,{})
        self.assertEqual(entry.summary,'Meaning.')
        self.assertEqual(entry.fields,(('x','Useful value.'),))
        with self.assertRaisesRegex(ValueError,'Missing semantic field'):
            DocEntry.from_row('zipzi.A',dict(row,fields=[('x','')]),{})

    def test_dependency_components_are_deterministic_and_detect_cycles(self):
        definitions = {'crate::A': record('crate::A',[field('b',{'kind':'ref','name':'crate::B'})]),
                       'crate::B': record('crate::B',[field('a',{'kind':'ref','name':'crate::A'})]),
                       'crate::C': record('crate::C')}
        graph=domain_dependencies(definitions,{'crate::A':'mesh','crate::B':'scene','crate::C':'common'})
        self.assertEqual(graph,{'common':(), 'mesh':('scene',), 'scene':('mesh',)})
        self.assertEqual(strong_components(graph),(('common',),('mesh','scene')))


if __name__ == '__main__':
    unittest.main()
