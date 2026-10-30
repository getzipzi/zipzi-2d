#!/usr/bin/env python3
"""Measure Python SDK generated sources without importing the SDK or native code."""
import argparse
import ast
from collections import Counter
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / 'crates/zipzi-python2'


def inspect(package):
    package = Path(package)
    paths = [package / 'schema/authoring.json', *sorted((package / 'schema/types').glob('*.json')),
             *sorted((package / 'client/python/zipzi/_generated').rglob('*.py')),
             *sorted((package / 'src/generated').rglob('*.rs')),
             *sorted((package / 'client/python/zipzi/docs').rglob('reference*.md')),
             *sorted((package / 'client/python/zipzi/docs/reference').rglob('*.md'))]
    files = {}
    for path in sorted(set(paths)):
        if not path.is_file():
            continue
        data = path.read_bytes()
        files[path.relative_to(package).as_posix()] = {'bytes': len(data), 'lines': len(data.splitlines())}
    values = list((package / 'client/python/zipzi/_generated/values').glob('*.py'))
    modeling = package / 'client/python/zipzi/_generated/modeling.py'
    source_paths = ([modeling] if modeling.exists() else []) + values
    classes = invalid_setattrs = assigned = 0
    for path in source_paths:
        tree = ast.parse(path.read_text(encoding='utf-8'))
        nodes = [n for n in tree.body if isinstance(n, ast.ClassDef)]
        classes += len(nodes)
        if path in values:
            assigned += len(nodes)
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call) or len(node.args) != 3:
                continue
            func, obj, name, value = node.func, *node.args
            if (isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name)
                    and func.value.id == 'object' and func.attr == '__setattr__'
                    and isinstance(obj, ast.Name) and obj.id == 'self'
                    and isinstance(name, ast.Constant) and isinstance(value, ast.Attribute)
                    and isinstance(value.value, ast.Name) and value.value.id == 'self'
                    and name.value == value.attr):
                invalid_setattrs += 1
    catalog_file = package / 'client/python/zipzi/_generated/catalog.py'
    catalog = {}
    if catalog_file.exists():
        tree = ast.parse(catalog_file.read_text(encoding='utf-8'))
        for node in tree.body:
            if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id == 'CATALOG':
                catalog = ast.literal_eval(node.value)
    inherited = {}
    for method in ('summary', '__repr__'):
        docs = Counter(entry.get('documentation', '') for path, entry in catalog.items() if path.endswith('.' + method))
        inherited[method] = {'entries': sum(docs.values()), 'unique_descriptions': len(docs),
                             'duplicate_descriptions': sum(count - 1 for count in docs.values())}
    repeated_fields = sum(bool(entry.get('fields')) and 'Fields:\n' in entry.get('documentation', '') for entry in catalog.values())
    over_review_threshold = {path: info['lines'] for path, info in files.items()
                             if path.endswith(('.py', '.rs', '.md')) and info['lines'] > 2000}
    return {'files': files, 'total_bytes': sum(f['bytes'] for f in files.values()),
            'max_file_bytes': max((f['bytes'] for f in files.values()), default=0),
            'max_file_lines': max((f['lines'] for f in files.values()), default=0),
            'files_over_2000_lines': over_review_threshold,
            'modeling_classes': classes, 'domain_assigned_classes': assigned,
            'domain_coverage': assigned / classes if classes else None,
            'domain_coverage_definition': 'Classes declared in generated values domain modules / generated modeling and values classes',
            'invalid_self_setattrs': invalid_setattrs, 'catalog_entries': len(catalog),
            'inherited_method_docs': inherited, 'entries_with_repeated_field_blocks': repeated_fields}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, default=PACKAGE)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/python2/codegen-readability/stats.json')
    args = parser.parse_args()
    report = inspect(args.package)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    print(f'Wrote {args.output}: {report["total_bytes"]} bytes, {report["modeling_classes"]} classes')


if __name__ == '__main__':
    main()
