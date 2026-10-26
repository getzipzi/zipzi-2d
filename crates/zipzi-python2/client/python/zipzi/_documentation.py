"""Offline public API descriptions; importing or describing never starts a host."""
from dataclasses import dataclass
from ._presentation import CompactValue

@dataclass(frozen=True,repr=False)
class ApiDescription(CompactValue):
    'One installed offline API entry; it describes the client package rather than probing host support.\n\nFields:\npath: Canonical public API path under zipzi.\nkind: Directory entry category such as module, type, callable or value.\nsignature: Rendered public signature; empty for entries without a callable/type signature.\ndocumentation: Semantic docstring for this installed entry.\nfields: Ordered pairs of public field name and semantic description; empty when no value fields are declared.\nchildren: Canonical child entry paths for further bounded describe queries.\nexample: Package-relative workflow/example reference; empty when no example is assigned.'
    __module__='zipzi'
    path:str
    kind:str
    signature:str
    documentation:str
    fields:tuple[tuple[str,str],...]=()
    children:tuple[str,...]=()
    example:str=''


def describe(path:str)->ApiDescription:
    """Look up an exact public path, e.g. Session.meshes.list, without IO or eval.

    Unknown paths raise KeyError. Short paths are normalized under zipzi.
    The root description links the bundled short guide and executable recipes.
    This describes the installed API, while runtime.capabilities() describes
    the connected host. No resource is constructed or RPC sent by this query.
    """
    from ._generated.catalog import CATALOG
    if not isinstance(path,str):raise TypeError('Expected a public API path')
    key=path if path=='zipzi' or path.startswith('zipzi.') else 'zipzi.'+path
    raw=CATALOG[key]
    return ApiDescription(key,raw['kind'],raw.get('signature',''),raw.get('documentation',''),tuple(raw.get('fields',())),tuple(raw.get('children',())),raw.get('example',''))
