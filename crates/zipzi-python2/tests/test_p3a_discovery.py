"""P3A-3/4/5 focused regressions, staged for the later combined acceptance run."""
from pathlib import Path
import sys
import unittest
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
                for _ in range(40):runtime.sessions.create().close()
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

if __name__=='__main__':unittest.main()
