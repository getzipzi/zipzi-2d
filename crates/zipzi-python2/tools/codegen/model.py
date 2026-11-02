"""Checked type graph and documentation records shared by code generators."""
from dataclasses import dataclass
from typing import Any
import ast

from . import dependencies, normalize, projections


@dataclass(frozen=True)
class DocRule:
    key: str
    title: str
    documentation: str
    source: str
    rule_refs: tuple[str, ...] = ()


@dataclass(frozen=True)
class DocBinding:
    """Documentation attached to a generator descriptor, before docstring rendering."""
    summary: str
    rule_refs: tuple[str, ...] = ()
    example: str | None = None


def source_rule(package, module: str, name: str) -> DocRule:
    tree = ast.parse((package / 'client/python/zipzi' / (module+'.py')).read_text(encoding='utf-8'))
    node = next(n for n in tree.body if isinstance(n, ast.Assign)
                and any(isinstance(t, ast.Name) and t.id == name for t in n.targets))
    return DocRule(**ast.literal_eval(node.value), source='python:zipzi.'+module+'.'+name)


def collection_rules(package) -> dict[str, DocRule]:
    items = (source_rule(package, '_facade_support', '_COLLECTION_READ_DOCUMENTATION'),
             source_rule(package, '_facade_support', '_COLLECTION_PAGE_DOCUMENTATION'),
             source_rule(package, '_lifecycle', '_EDIT_WRITE_DOCUMENTATION'))
    return {rule.key: rule for rule in items}


def validate_rules(rules: dict[str, DocRule], refs: tuple[str, ...]) -> None:
    if len(refs) != len(set(refs)):
        raise ValueError('Duplicate documentation rule reference')
    for key in refs:
        if key not in rules:
            raise ValueError('Missing documentation rule: '+key)
        rule = rules[key]
        if rule.key != key or not all((rule.source, rule.title, rule.documentation.strip())):
            raise ValueError('Invalid documentation rule: '+key)
        # Rules are leaves. References and recursive expansion are deliberately
        # unsupported; a rule cannot depend on another rule (or itself).
        if rule.rule_refs:
            raise ValueError('Documentation rules must be leaves; recursive references are unsupported: '+key)


def full_documentation(binding: DocBinding, rules: dict[str, DocRule]) -> str:
    validate_rules(rules, binding.rule_refs)
    return '\n\n'.join((binding.summary, *(rules[key].documentation for key in binding.rule_refs)))


@dataclass(frozen=True)
class Projection:
    """An emitted Python implementation and its reviewed public paths."""
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
        imports = dependencies.domain_dependencies(definitions, domains)
        return cls(root, definitions, domains, imports, dependencies.strong_components(imports))

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
    identity: str | None = None
    type_refs: tuple[str, ...] = ()
    documentation_source: str = ''
    rule_refs: tuple[str, ...] = ()
    field_roles: tuple[tuple[str, str, str], ...] = ()
    diagnostics: tuple[tuple[str, str, str], ...] = ()

    @classmethod
    def from_row(cls, path: str, row: dict[str, Any], common: dict[str, str]) -> 'DocEntry':
        fields = tuple(tuple(pair) for pair in row['fields']) if 'fields' in row else None
        if fields and any(not name or not text.strip() for name, text in fields):
            raise ValueError('Missing semantic field documentation: '+path)
        summary = row.get('documentation', '')
        if fields:
            summary = '' if summary.startswith('Fields:\n') else summary.partition('\n\nFields:\n')[0]
        if row['kind'] == 'callable' and not summary.strip():
            raise ValueError('Missing semantic callable documentation: '+path)
        method = path.rsplit('.', 1)[-1]
        inherited = method if method in common and summary == common[method] else None
        return cls(path, row['kind'], row.get('signature', ''), summary, fields,
                   tuple(sorted(set(row.get('children', ())))), row.get('example'), inherited, row.get('_identity'), tuple(row.get('type_refs', ())), row.get('_documentation_source', ''),
                   tuple(row.get('rule_refs', ())), tuple(row.get('field_roles', ())), tuple(row.get('_diagnostics', ())))

    def row(self) -> dict[str, Any]:
        result: dict[str, Any] = {'kind': self.kind, 'signature': self.signature,
                                  'documentation': self.summary, 'children': self.children}
        if self.type_refs:result['type_refs'] = self.type_refs
        if self.rule_refs:result['rule_refs'] = self.rule_refs
        if self.field_roles:result['field_roles'] = self.field_roles
        if self.fields is not None:
            result['fields'] = list(self.fields)
        if self.example is not None:
            result['example'] = self.example
        return result
