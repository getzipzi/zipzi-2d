"""Normalize compiler-exported shapes into a checked graph of Rust type IDs."""
from copy import deepcopy

NAMED_KINDS = frozenset({'record', 'serde_record', 'union', 'serde_enum', 'opaque'})


def _named(value):
    return (isinstance(value, dict) and isinstance(value.get('kind'), str)
            and value['kind'] in NAMED_KINDS and isinstance(value.get('name'), str)
            and value['name'] != '__skipped')


def normalize(contract, externals):
    """Return (root, definitions); external opaque identities must be declared.

    Nested occurrences become references. An opaque occurrence retains a hint for
    the temporary legacy renderer adapter, while the referenced definition is the
    complete one when available. Field and variant order stays untouched.
    """
    candidates = {}
    opaque = set()

    def collect(value):
        if isinstance(value, dict):
            if _named(value):
                name = value['name']
                if value['kind'] == 'opaque':
                    opaque.add(name)
                else:
                    candidates.setdefault(name, []).append(value)
            for child in value.values():
                collect(child)
        elif isinstance(value, list):
            for child in value:
                collect(child)

    def refs(value):
        if _named(value):
            ref = {'kind': 'ref', 'name': value['name']}
            if value['kind'] == 'opaque':
                ref['opaque'] = True
            return ref
        if isinstance(value, dict):
            return {key: refs(child) for key, child in value.items()}
        if isinstance(value, list):
            return [refs(child) for child in value]
        return value

    collect(contract)
    definitions = {}
    for name, shapes in sorted(candidates.items()):
        normalized = [{key: refs(value) for key, value in shape.items()} for shape in shapes]
        if any(shape != normalized[0] for shape in normalized[1:]):
            raise ValueError('Conflicting complete type definitions: '+name)
        definitions[name] = normalized[0]
    for name in sorted(opaque - definitions.keys()):
        if name not in externals:
            raise ValueError('Unresolved opaque type: '+name)
        definitions[name] = {'kind': 'opaque', 'name': name}
    root = refs(contract)

    state = {}
    def validate(value):
        if isinstance(value, dict):
            if value.get('kind') == 'ref':
                name = value.get('name')
                if name not in definitions:
                    raise ValueError('Unresolved type reference: '+str(name))
                if state.get(name) == 1:
                    raise ValueError('Cyclic type definition: '+name)
                if state.get(name) != 2:
                    state[name] = 1
                    validate(definitions[name])
                    state[name] = 2
                return
            for child in value.values():
                validate(child)
        elif isinstance(value, list):
            for child in value:
                validate(child)
    validate(root)
    for name in definitions:
        if name not in state:
            state[name] = 1
            validate(definitions[name])
            state[name] = 2
    return root, definitions


def expand(root, definitions):
    """Temporary adapter for legacy renderers; remove after ref consumers migrate."""
    def visit(value):
        if isinstance(value, dict):
            if value.get('kind') == 'ref':
                name = value['name']
                if value.get('opaque'):
                    return {'kind': 'opaque', 'name': name}
                return visit(definitions[name])
            return {key: visit(child) for key, child in value.items()}
        if isinstance(value, list):
            return [visit(child) for child in value]
        return deepcopy(value)
    return visit(root)
