"""Constant-depth presentation of owned values; never requests or clones payloads."""
from collections.abc import Mapping, Sequence
from typing import cast
from dataclasses import fields, is_dataclass
from itertools import islice


def _short(value: object, depth: int = 0) -> str:
    if isinstance(value, (bytes, bytearray, memoryview)):
        value=cast(bytes | bytearray | memoryview[int],value)
        return f'<{type(value).__name__} {len(value)} bytes>'
    if isinstance(value,str):
        return repr(value[:80])+('…' if len(value)>80 else '')
    if type(value) is int and value.bit_length()>256:return f"<int {value.bit_length()} bits>"
    if value is None or type(value) in (bool,int,float):return repr(value)
    if isinstance(value,(tuple,list)):
        value=cast(Sequence[object],value)
        if depth>=2:return f'<{type(value).__name__} {len(value)} items>'
        values=', '.join(_short(value[i],depth+1) for i in range(min(2,len(value))))
        return f'({values}'+(f', …; {len(value)} items' if len(value)>2 else '')+')'
    if isinstance(value,Mapping):
        value=cast(Mapping[object,object],value)
        if depth>=2:return f'<mapping {len(value)} items>'
        entries=', '.join(_short(k,depth+1)+': '+_short(value[k],depth+1) for k in islice(value,2))
        return '{'+entries+(f', …; {len(value)} items' if len(value)>2 else '')+'}'
    if is_dataclass(value) and not isinstance(value,type):
        if depth>=2:return type(value).__name__+'(…)'
        selected=[f for f in fields(value) if f.repr and not f.name.startswith('_')]
        values=', '.join(f.name+'='+_short(getattr(value,f.name),depth+1) for f in selected[:5])
        return type(value).__name__+'('+values+(', …' if len(selected)>5 else '')+')'
    return '<'+type(value).__name__+'>'


class CompactValue:
    """Owned-value display capped at 1,024 characters, without IO or full scans."""
    def __repr__(self)->str:
        """Return a local summary capped at 1,024 characters, with no RPC/full scan.

        Large arrays and bytes show bounded samples/counts. Explicit field access
        still exposes the owned full value; no host resource is created/released.
        """
        text=_short(self)
        return text if len(text)<=1024 else text[:1023]+'…'
    def summary(self)->str:
        """Return a bounded, local summary; full fields remain explicitly accessible."""
        return self.__repr__()
