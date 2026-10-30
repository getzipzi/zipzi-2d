"""Checked type graph and documentation records shared by code generators."""
from dataclasses import dataclass
import re
from typing import Any

from . import dependencies, normalize, projections


@dataclass(frozen=True)
class Projection:
    """Reviewed target ownership; destination modules are materialized in P3."""
    rust_type_id: str
    projection_id: str
    domain: str
    implementation_module: str
    canonical_path: str
    aliases: tuple[str, ...] = ()
    construction: str = 'compiled-shape'
    field_conversion: str = 'compiled-shape'
    wire: str = 'compiled-shape'
    reason: str | None = None


def validate_projections(items: tuple[Projection, ...]) -> None:
    identities: set[tuple[str, str]] = set()
    paths: set[str] = set()
    for item in items:
        if item.projection_id != 'canonical' and not item.reason:
            raise ValueError('Special projection requires a reason: '+item.rust_type_id)
        key = (item.rust_type_id, item.projection_id)
        if key in identities:
            raise ValueError('Duplicate projection: '+item.rust_type_id)
        identities.add(key)
        for path in (item.canonical_path, *item.aliases):
            if path in paths:
                raise ValueError('Conflicting public projection: '+path)
            paths.add(path)


@dataclass(frozen=True)
class TypeGraph:
    root: Any
    definitions: dict[str, dict[str, Any]]
    domains: dict[str, str]
    projections: tuple[Projection, ...]
    dependencies: dict[str, tuple[str, ...]]
    components: tuple[tuple[str, ...], ...]

    @classmethod
    def from_contract(cls, contract: Any) -> 'TypeGraph':
        if not isinstance(contract, dict) or contract.get('format_version') != 2:
            raise ValueError('Unsupported schema format')
        root, definitions = contract['root'], contract['definitions']
        normalize.validate_graph(root, definitions, projections.EXTERNAL_TYPES)
        complete = {name for name, shape in definitions.items() if shape['kind'] != 'opaque'}
        missing = sorted(complete - projections.OWNERS.keys())
        if missing:
            raise ValueError('Missing domain projection: '+', '.join(missing))
        domains = {name: projections.OWNERS[name] for name in sorted(complete)}
        def public_name(identity: str) -> str:
            if '<' in identity:
                return ''.join(re.findall(r'(?:^|::)([A-Z][A-Za-z0-9_]*)', identity))
            return identity.rsplit('::', 1)[-1]
        selected = tuple(Projection(identity, 'canonical', domain,
                                    'zipzi._generated.values.'+domain,
                                    'zipzi.modeling.'+domain+'.'+public_name(identity))
                         for identity, domain in domains.items())
        validate_projections(selected)
        imports = dependencies.domain_dependencies(definitions, domains)
        return cls(root, definitions, domains, selected, imports, dependencies.strong_components(imports))

    def definition(self, identity: str) -> dict[str, Any]:
        try:
            return self.definitions[identity]
        except KeyError as error:
            raise ValueError('Unresolved type reference: '+identity) from error

    def resolve(self, shape: dict[str, Any]) -> dict[str, Any]:
        if shape.get('kind') != 'ref':
            return shape
        if shape.get('opaque'):
            return {'kind': 'opaque', 'name': shape['name']}
        return self.definition(shape['name'])



@dataclass(frozen=True)
class DocEntry:
    path: str
    kind: str
    signature: str
    summary: str
    fields: tuple[tuple[str, str], ...] | None
    children: tuple[str, ...]
    example: str | None
    inherited: str | None = None
    identity: int | None = None

    @classmethod
    def from_row(cls, path: str, row: dict[str, Any], common: dict[str, str]) -> 'DocEntry':
        fields = tuple(tuple(pair) for pair in row['fields']) if 'fields' in row else None
        if fields and any(not name or not text.strip() for name, text in fields):
            raise ValueError('Missing semantic field documentation: '+path)
        summary = row.get('documentation', '')
        if fields:
            summary = summary.partition('\n\nFields:\n')[0]
        if row['kind'] == 'callable' and not summary.strip():
            raise ValueError('Missing semantic callable documentation: '+path)
        method = path.rsplit('.', 1)[-1]
        inherited = method if method in common and summary == common[method] else None
        return cls(path, row['kind'], row.get('signature', ''), summary, fields,
                   tuple(sorted(set(row.get('children', ())))), row.get('example'), inherited, row.get('_identity'))

    def row(self) -> dict[str, Any]:
        result: dict[str, Any] = {'kind': self.kind, 'signature': self.signature,
                                  'documentation': self.summary, 'children': self.children}
        if self.fields is not None:
            result['fields'] = list(self.fields)
        if self.example is not None:
            result['example'] = self.example
        return result
