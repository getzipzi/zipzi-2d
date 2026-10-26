"""A client/process restart preserves host Runtime identity; another host does not."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'tools/python'))
from python2_agent_review import native_lifecycle,ordinary_failure


class NativeLifecycleReview(unittest.TestCase):
    def test_unified_apply_has_the_same_ordinary_failure_contract_as_the_facade(self):
        event={'kind':'public_exception','path':'Edit.apply','code':'STALE_FIELD',
               'edit_state':'aborted','edit_identity':7,'domain':None}
        self.assertTrue(ordinary_failure([event]))
        self.assertFalse(ordinary_failure([dict(event,code='STALE_PLAN')]))
        self.assertFalse(ordinary_failure([dict(event,edit_state='editing')]))

    def events(self,second_host=False):
        rows=[]
        for process in (1,2):
            runtime='host-b' if second_host and process==2 else 'host-a'
            info={'id':{'runtime':runtime,'serial':0}}
            rows.append({'kind':'registry','pid':process,'runtime':process,'phase':'before_creations','sessions':[info]})
            for index in range(20):
                identity={'runtime':runtime,'serial':process*100+index}
                rows.append({'kind':'public_return','path':'Sessions.create','return_type':'Session','pid':process,'runtime_identity':process,'value':{'session_id':identity}})
                rows.append({'kind':'public_return','path':'Session.close','return_type':'SessionInfo','pid':process,'value':{'id':identity}})
            rows.append({'kind':'public_return','path':'Session.close','return_type':'SessionInfo','pid':process,'value':info})
            rows.append({'kind':'registry','pid':process,'runtime':process,'phase':'before_runtime_close','sessions':[info]})
        return rows

    def test_two_clients_on_one_host_complete_one_lifecycle_group(self):
        checks,details=native_lifecycle(self.events())
        self.assertTrue(all(checks.values()));self.assertEqual(details['actual_created_sessions'],40)

    def test_separate_native_runtimes_cannot_be_combined_to_reach_forty(self):
        checks,_=native_lifecycle(self.events(second_host=True))
        self.assertFalse(checks['forty_sessions_created'])

    def test_a_created_session_left_resident_fails_close_and_registry_checks(self):
        rows=self.events();rows=[e for e in rows if not (e.get('path')=='Session.close' and e['value']['id']['serial']==219)]
        checks,_=native_lifecycle(rows)
        self.assertFalse(checks['forty_sessions_closed']);self.assertFalse(checks['registry_restored'])

    def test_public_recovery_helpers_preserve_the_logical_operation(self):
        rows=self.events()
        for event in rows:
            if event.get('path') in {'Sessions.create','Session.close'}:
                operation=event['path'];event['path']='recover_call';event['inputs']={'method':{'callable':operation}}
        checks,_=native_lifecycle(rows)
        self.assertTrue(all(checks.values()))


if __name__=='__main__':unittest.main()
