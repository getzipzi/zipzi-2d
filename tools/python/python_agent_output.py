"""Bound each delivered shell result, retaining complete command output externally."""
from __future__ import annotations
import hashlib
import json
from uuid import uuid4


class OutputPages:
    def __init__(self,limit=16384):self.limit=limit;self.cursors={}

    def first(self,result):return self._page(result,0)

    def read(self,cursor):
        if cursor not in self.cursors:raise ValueError('Unknown output cursor for this run')
        result,offset=self.cursors[cursor]
        return self._page(result,offset)

    def _page(self,result,offset):
        output=result['output'];cursor=str(uuid4())
        body={key:result[key] for key in ('returncode','elapsed_seconds')}
        body.update(total_output_bytes=len(output.encode()),output_sha256=hashlib.sha256(output.encode()).hexdigest())
        def candidate(length):
            end=offset+length
            return dict(body,output=output[offset:end],output_complete=end==len(output),output_cursor=None if end==len(output) else cursor,
                        read_output_command=None if end==len(output) else 'read-output '+cursor)
        lower,upper=0,min(len(output)-offset,self.limit)
        while lower<upper:
            middle=(lower+upper+1)//2
            if len(json.dumps(candidate(middle),ensure_ascii=False).encode())<=self.limit:lower=middle
            else:upper=middle-1
        if lower==0 and offset<len(output):raise ValueError('Output metadata exceeds the delivery budget')
        page=candidate(lower)
        if page['output_cursor'] is not None:self.cursors[cursor]=(result,offset+lower)
        return page
