"""Normalize compiler-exported shapes into a checked graph of Rust type IDs."""
NAMED_KINDS = frozenset({'record', 'serde_record', 'union', 'serde_enum', 'opaque'})


def _named(value):
    return (isinstance(value, dict) and isinstance(value.get('kind'), str)
            and value['kind'] in NAMED_KINDS and isinstance(value.get('name'), str)
            and value['name'] != '__skipped')


def validate_graph(root, definitions, externals):
    """Check a compiler-exported graph without expanding its references."""
    if not isinstance(definitions, dict):
        raise ValueError('Invalid type definitions')
    for name, shape in definitions.items():
        if not isinstance(name, str) or not _named(shape) or shape['name'] != name:
            raise ValueError('Invalid type definition: '+str(name))
        if shape['kind'] == 'opaque' and name not in externals:
            raise ValueError('Unresolved opaque type: '+name)
    state = {}
    def validate(value, definition=False):
        if isinstance(value, dict):
            if value.get('kind') == 'ref':
                name = value.get('name')
                if name not in definitions:
                    raise ValueError('Unresolved type reference: '+str(name))
                if state.get(name) == 1:
                    raise ValueError('Cyclic type definition: '+name)
                if state.get(name) != 2:
                    state[name] = 1
                    validate(definitions[name], definition=True)
                    state[name] = 2
                return
            if _named(value) and not definition:
                raise ValueError('Nested named type must be a reference: '+value['name'])
            for child in value.values():
                validate(child)
        elif isinstance(value, list):
            for child in value:
                validate(child)
    validate(root)
    for name in definitions:
        if name not in state:
            state[name] = 1
            validate(definitions[name], definition=True)
            state[name] = 2
