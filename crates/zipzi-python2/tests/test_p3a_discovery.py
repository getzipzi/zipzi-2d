"""P3A-3/4/5 focused regressions, staged for the later combined acceptance run."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parent))
import test_api as support
zipzi,meshes=support.zipzi,support.meshes

class P3ADiscovery(unittest.TestCase):
    setUp=support.PythonSdk.setUp
    runtime=support.PythonSdk.runtime
    def test_metadata_pages_keep_storage_order_duplicates_and_source(self):
        for mode in ('local','connected'):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode);session=runtime.sessions.create()
                ids=tuple(support.uid(30000+n) for n in range(257))
                for start in range(0,len(ids),64):
                    with session.edit('metadata fixture') as edit:
                        edit.apply_many(tuple(meshes.Create(meshes.MeshSpec('Same',support.GEOMETRY,id=id)) for id in ids[start:start+64]))
                page=session.meshes.list(name='Same',limit=128);source=page.source
                self.assertIsNotNone(page.next_cursor)
                found=list(page.items)
                while page.next_cursor is not None:
                    page=session.meshes.list(name='Same',limit=128,cursor=page.next_cursor)
                    self.assertEqual(page.source,source);found.extend(page.items)
                self.assertEqual(tuple(item.id for item in found),ids)
                selected=session.meshes.get_many((ids[-1],ids[0],ids[-1]),expected=source)
                self.assertEqual(tuple(item.id for item in selected.items),(ids[-1],ids[0],ids[-1]))
                old=session.meshes.list(limit=1)
                with session.edit('source changes') as edit:edit.meshes.rename(ids[0],'Next')
                for read in (lambda:session.meshes.list(cursor=old.next_cursor),lambda:session.meshes.get(ids[0],expected=source)):
                    with self.assertRaises(zipzi.SdkFailure) as stale:read()
                    self.assertEqual(stale.exception.code,'STALE_SOURCE')
                session.close(discard_modified=True)
    def test_session_alias_detach_close_and_default_slot_reuse(self):
        for mode in ('local','connected'):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                for _ in range(40):runtime.sessions.create().close(discard_modified=True)
                session=runtime.sessions.create();alias=runtime.sessions.attach(session.id)
                session.detach()
                with self.assertRaises(zipzi.SdkFailure):alias.info()
                rebound=runtime.sessions.attach(session.id)
                with rebound.edit('modified') as edit:edit.meshes.create(meshes.MeshSpec('Face',support.GEOMETRY))
                with self.assertRaises(zipzi.SdkFailure) as modified:rebound.close()
                self.assertEqual(modified.exception.code,'MODIFIED')
                closed=rebound.close(discard_modified=True)
                self.assertEqual(closed.state,'closed');self.assertEqual(rebound.close(),closed)
                self.assertEqual(runtime.sessions.list(),())
    def test_offline_catalog_and_large_owned_value_representation(self):
        entry=zipzi.describe('Session.meshes.list')
        self.assertIn('Page',entry.signature)
        geometry=meshes.Geometry(tuple(range(1000)),tuple((0.,0.) for _ in range(1000)),tuple((0.,0.) for _ in range(1000)),())
        self.assertLessEqual(len(repr(geometry)),1024)
        self.assertEqual(geometry.summary(),repr(geometry))
        self.assertEqual(len(geometry.positions),1000)
        self.assertIn('vertex_ids',geometry.__doc__)

    def test_offline_catalog_aliases_fields_and_cycle_detection(self):
        from zipzi._generated.catalog import index
        alias=zipzi.describe('modeling.PhysicsAssetProperty')
        original=zipzi.describe('modeling.PhysicsAssetPropertyFps')
        self.assertEqual(alias.path,'zipzi.modeling.PhysicsAssetProperty')
        self.assertEqual(alias.fields,original.fields)
        self.assertIn('zipzi.modeling.PhysicsAssetProperty.summary',alias.children)
        self.assertEqual(zipzi.describe(alias.path+'.summary').documentation,
                         zipzi.describe(original.path+'.summary').documentation)
        for name,text in alias.fields:
            self.assertEqual(alias.format().count(name+': '+text),1)
        with self.assertRaises(KeyError):zipzi.describe('modeling.missing')
        with patch.object(index,'_data',return_value={'zipzi.a':{'alias':'zipzi.b'},'zipzi.b':{'alias':'zipzi.a'}}):
            with self.assertRaisesRegex(ValueError,'alias cycle'):index.lookup('zipzi.a')

    def test_candidate_cursor_undo_and_wrong_collection_fail_without_aborting_reads(self):
        for mode in ('local','connected'):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode);session=runtime.sessions.create()
                with session.edit('base') as edit:
                    first=edit.meshes.create(meshes.MeshSpec('Same',support.GEOMETRY))
                    second=edit.meshes.create(meshes.MeshSpec('Same',support.GEOMETRY))
                formal=session.meshes.list(limit=1)
                with session.edit('candidate') as edit:
                    page=edit.meshes.list(limit=1)
                    edit.meshes.rename(second,'Changed')
                    with self.assertRaises(zipzi.SdkFailure) as stale:edit.meshes.list(limit=1,cursor=page.next_cursor)
                    self.assertEqual(stale.exception.code,'STALE_SOURCE')
                    edit.meshes.rename(first,'Still editable')
                session.undo().wait()
                with self.assertRaises(zipzi.SdkFailure) as stale:session.meshes.list(limit=1,cursor=formal.next_cursor)
                self.assertEqual(stale.exception.code,'STALE_SOURCE')
                current=session.meshes.list(limit=1)
                with self.assertRaises(zipzi.SdkFailure):session.parameters.list(limit=1,cursor=current.next_cursor)
                self.assertEqual(session.meshes.get(first).value.name,'Same')

    def test_recovered_page_keeps_captured_source_after_peer_publication(self):
        runtime=self.runtime('connected',drop='collection_page');session=runtime.sessions.create()
        with session.edit('base') as edit:
            first=edit.meshes.create(meshes.MeshSpec('First',support.GEOMETRY))
            edit.meshes.create(meshes.MeshSpec('Second',support.GEOMETRY))
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:session.meshes.list(limit=1)
        with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
            attached=peer.sessions.attach(session.id)
            with attached.edit('peer') as edit:edit.meshes.rename(first,'Peer')
        page=lost.exception.recover()
        self.assertIsInstance(page,zipzi.Page)
        self.assertEqual(page.items[0].name,'First')
        self.assertNotEqual(page.source,session.meshes.get(first).source)
        with self.assertRaises(zipzi.SdkFailure) as stale:session.meshes.list(limit=1,cursor=page.next_cursor)
        self.assertEqual(stale.exception.code,'STALE_SOURCE')

    def test_close_and_detach_lost_responses_keep_shared_binding_owner(self):
        for operation in ('close_session','detach_session'):
            with self.subTest(operation=operation):
                runtime=self.runtime('connected',drop=operation);session=runtime.sessions.create()
                alias=runtime.sessions.attach(session.id)
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    if operation=='close_session':session.close(discard_modified=True)
                    else:session.detach()
                self.assertTrue(session._binding.active)
                result=lost.exception.recover()
                self.assertFalse(alias._binding.active)
                if operation=='close_session':
                    self.assertEqual(result.state,'closed');self.assertEqual(session.close(),result)
                    self.assertEqual(runtime.sessions.list(),())
                else:
                    self.assertIsNone(result)
                    attached=runtime.sessions.attach(session.id)
                    self.assertIsNot(attached._binding,alias._binding)
                    self.assertEqual(attached.info().id,session.id)
                with self.assertRaises(zipzi.SdkFailure):alias.info()

    def test_other_clients_active_edits_paths_and_detached_refs_are_protected(self):
        runtime=self.runtime('connected');session=runtime.sessions.create()
        with session.edit('base') as edit:face=edit.meshes.create(meshes.MeshSpec('Face',support.GEOMETRY))
        reference=session.reference('mesh',face)
        with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
            attached=peer.sessions.attach(session.id)
            before=session.info()
            with self.assertRaises(zipzi.Busy):session.close(discard_modified=True)
            self.assertEqual(session.info(),before)
            attached.detach()
        candidate=session.edit('active')
        with self.assertRaises(zipzi.Busy):session.close(discard_modified=True)
        candidate.cancel()
        session.detach()
        rebound=runtime.sessions.attach(session.id)
        with self.assertRaises(zipzi.SdkFailure):reference.resolve()
        self.assertEqual(rebound.meshes.get(face).value.name,'Face')
        with runtime.resources.prepare_buffer(b'independent') as buffer:
            rebound.close(discard_modified=True)
            self.assertEqual(buffer.read(),b'independent')

if __name__=='__main__':unittest.main()
