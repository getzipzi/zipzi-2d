"""Offline public API descriptions; importing or describing never starts a host."""
from dataclasses import dataclass
from functools import lru_cache
from ._presentation import CompactValue

@dataclass(frozen=True,repr=False)
class ApiDescription(CompactValue):
    'One installed offline API entry; it describes the client package rather than probing host support.\n\nFields:\npath: Canonical public API path under zipzi.\nkind: Directory entry category such as module, type, callable or value.\nsignature: Rendered public signature; empty for entries without a callable/type signature.\ndocumentation: Entry-specific semantics; rules contains the directly referenced shared explanations.\nfields: Ordered pairs of public field name and semantic description; empty when no value fields are declared.\nchildren: Canonical child entry paths for further bounded describe queries.\ntype_refs: Referenced public type paths resolved from source annotations.\nexample: Package-relative workflow/example reference; empty when no example is assigned.\nrule_refs: Direct shared rule identities; rules are leaves and never query children or referenced types.\nrules: Resolved title/text pairs for the directly referenced rules, expanded once by format().\nfield_roles: Field name, projection display role and fixed value for explicitly declared projections.'
    __module__='zipzi'
    path:str
    kind:str
    signature:str
    documentation:str
    fields:tuple[tuple[str,str],...]=()
    children:tuple[str,...]=()
    example:str=''
    type_refs:tuple[str,...]=()
    rule_refs:tuple[str,...]=()
    rules:tuple[tuple[str,str],...]=()
    field_roles:tuple[tuple[str,str,str],...]=()

    def format(self) -> str:
        """Return complete help text without truncation or IO.

        Use print(description.format()) for full help. repr, str and summary
        remain bounded previews. Direct shared rules expand once; children and
        type references remain paths and are never recursively queried.
        """
        lines=[self.path+' ('+self.kind+')']
        if self.signature:lines.extend(('',self.signature))
        if self.documentation:lines.extend(('',self.documentation))
        if self.fields:
            lines.extend(('', 'Fields:'))
            lines.extend(name+': '+text for name,text in self.fields)
        for title,text in self.rules:
            lines.extend(('',title+':',text))
        if self.children:
            lines.extend(('', 'Children:', *self.children))
        if self.type_refs:lines.extend(('', 'Types:', *self.type_refs))
        if self.example:lines.extend(('', 'Example: '+self.example))
        return '\n'.join(lines)


def describe(path:str)->ApiDescription:
    """Look up an exact public path, e.g. Session.meshes.list, without IO or eval.

    Unknown paths raise KeyError. Short paths are normalized under zipzi.
    The root description links the bundled short guide and executable recipes.
    Use print(describe(path).format()) for complete help; direct printing is a
    bounded preview. Fields and child paths are also available individually.
    This describes the installed API, while runtime.capabilities() describes
    the connected host. No resource is constructed or RPC sent by this query.
    """
    if not isinstance(path,str):raise TypeError('Expected a public API path')
    key=path if path=='zipzi' or path.startswith('zipzi.') else 'zipzi.'+path
    return _describe(key)


@lru_cache(maxsize=512)
def _describe(key:str)->ApiDescription:
    from ._generated.catalog import lookup
    raw=lookup(key)
    return ApiDescription(key,raw['kind'],raw.get('signature',''),raw.get('documentation',''),tuple(raw.get('fields',())),tuple(raw.get('children',())),raw.get('example',''),tuple(raw.get('type_refs',())),raw.get('rule_refs',()),raw.get('rules',()),raw.get('field_roles',()))
