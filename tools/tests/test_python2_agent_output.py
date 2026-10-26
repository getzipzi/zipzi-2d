"""Delivered pages respect the byte budget without discarding original output."""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'tools/python'))
from python2_agent_output import OutputPages


class ShellOutputPages(unittest.TestCase):
    def test_unicode_and_escaped_output_round_trip_with_every_page_bounded(self):
        text=('帮助🙂\n"quoted"\\\t\x01'*2000)
        result={'command':'large command','returncode':0,'elapsed_seconds':.2,'output':text}
        pages=OutputPages();page=pages.first(result);parts=[]
        self.assertFalse(page['output_complete'])
        while True:
            self.assertLessEqual(len(json.dumps(page,ensure_ascii=False).encode()),16384)
            parts.append(page['output'])
            if page['output_complete']:break
            page=pages.read(page['output_cursor'])
        self.assertEqual(''.join(parts),text)
        self.assertEqual(result['output'],text)

    def test_empty_error_and_short_outputs_keep_exit_status(self):
        for text,code in [('',0),('failed',7)]:
            page=OutputPages().first({'command':'command','returncode':code,'elapsed_seconds':.1,'output':text})
            self.assertTrue(page['output_complete']);self.assertIsNone(page['output_cursor'])
            self.assertEqual(page['output'],text);self.assertEqual(page['returncode'],code)

    def test_output_cursors_are_private_to_the_tool_instance(self):
        pages=OutputPages();result={'command':'command','returncode':0,'elapsed_seconds':.1,'output':'x'*20000}
        page=pages.first(result)
        self.assertEqual(pages.read(page['output_cursor'])['output'],pages.read(page['output_cursor'])['output'])
        with self.assertRaises(ValueError):OutputPages().read(page['output_cursor'])


if __name__=='__main__':unittest.main()
