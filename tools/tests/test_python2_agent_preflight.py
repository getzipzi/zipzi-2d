"""Service availability can pass without optional model/service revision metadata."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/python'))
SPEC=importlib.util.spec_from_file_location('agent_packets',ROOT/'tools/python/python2_agent_tasks.py')
packets=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(packets)


class Python2AgentPreflight(unittest.TestCase):
    def test_completed_service_without_metadata_passes_cli_preflight(self):
        result=packets.preflight_identity([{'type':'turn.completed'}],0)
        with patch.object(sys,'argv',['agent_tasks','--preflight','unused-output']), \
             patch.object(packets,'preflight',return_value=result),patch('builtins.print'):
            packets.main()

    def test_service_failure_still_fails_cli_preflight(self):
        for events,code in (([],0),([{'type':'turn.completed'}],1)):
            result=packets.preflight_identity(events,code)
            with self.subTest(events=events,code=code), \
                 patch.object(sys,'argv',['agent_tasks','--preflight','unused-output']), \
                 patch.object(packets,'preflight',return_value=result),patch('builtins.print'):
                with self.assertRaises(SystemExit) as stopped:packets.main()
                self.assertEqual(stopped.exception.code,1)


if __name__=='__main__':unittest.main()
