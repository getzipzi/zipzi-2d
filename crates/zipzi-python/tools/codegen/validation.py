"""Primitive validation policy shared by inline checks and generated helpers."""

PRIMITIVES = {
    'string': 'str', 'bool': 'bool',
    'u8': 'int', 'u16': 'int', 'u32': 'int', 'u64': 'int', 'usize': 'int',
    'i32': 'int', 'i64': 'int', 'f32': 'float', 'f64': 'float',
}


def condition(kind, value, fast=False):
    if kind == 'string':
        return (f'type({value}) is not str and ' if fast else '')+f'not isinstance({value}, str)'
    if kind == 'bool':
        return f'type({value}) is not bool'
    if kind in {'f32', 'f64'}:
        numeric=f'isinstance({value}, bool) or not isinstance({value}, (float, int))'
        if fast:
            numeric=f'(type({value}) is not float and ({numeric}))'
        return (f'{numeric} or not math.isfinite({value})'
                + (f' or abs({value}) > 3.4028234663852886e38' if kind == 'f32' else ''))
    bits = {'u8': 8, 'u16': 16, 'u32': 32, 'u64': 64, 'usize': 64, 'i32': 32, 'i64': 64}[kind]
    low, high = (-(1 << (bits-1)), 1 << (bits-1)) if kind.startswith('i') else (0, 1 << bits)
    return f'type({value}) is not int or not {low} <= {value} < {high}'


def helpers():
    lines = []
    for kind, annotation in PRIMITIVES.items():
        lines.extend([f'def _check_{kind}(value: {annotation}) -> None:',
                      f'    if {condition(kind, "value", fast=True)}:',
                      f'        raise ValueError("Expected {kind}")', ''])
    return lines
