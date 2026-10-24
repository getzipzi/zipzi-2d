#!/usr/bin/env python3
"""Guard R2's ownership boundary; behavioral tests remain the publication proof."""
from __future__ import annotations

import re
from pathlib import Path

from runtime_inventory import mask_rust, closing

ROOT = Path(__file__).resolve().parents[2]


def collect(root: Path = ROOT) -> dict[str, str]:
    return {str(path.relative_to(root)): path.read_text()
            for crate in ('zipzi-editor-core', 'zipzi-editor')
            for path in sorted((root / 'crates' / crate / 'src').rglob('*.rs'))}


def violations(sources: dict[str, str]) -> list[str]:
    errors = []
    for name, source in sources.items():
        code = mask_rust(source)
        if re.search(r'\bdrain_events\s*\(', code):
            errors.append(f'{name}: only Runtime may drain hosted SDK events')
        if re.search(r'&\s*(?:\x27\w+\s+)?mut\s+(?:zipzi_sdk::)?AuthoringSession\b', code):
            errors.append(f'{name}: mutable formal SDK access bypasses Runtime')
        for match in re.finditer(r'\bstruct\s+(\w+)(?:<[^;{]*>)?\s*\{', code):
            body = code[match.end():closing(code, match.end()-1)]
            if re.search(r':\s*(?:zipzi_sdk::)?AuthoringSession\b', body):
                errors.append(f'{name}: {match[1]} owns a formal SDK session')
        if re.search(r'impl\s+(?:std::ops::)?DerefMut\s+for\s+(?:SessionRead|ModelRead)\b', code):
            errors.append(f'{name}: formal read guard must stay immutable')
        # Each detached native IO task returns formal ownership before attempting
        # the first weak-view callback. Match the async task, not dialog callbacks.
        if name.endswith(('/project.rs', '/benchmark.rs')):
            for match in re.finditer(r'\basync\s+move\s*\|(?:this|view),\s*cx\|\s*\{', code):
                body = code[match.end():closing(code, match.end()-1)]
                if not re.search(r'task\.run\s*\(', body):
                    continue
                publish = body.find('completion.publish(')
                callback = re.search(r'\b(?:this|view)\.update_in\s*\(', body)
                if publish < 0 or callback is None or publish > callback.start():
                    errors.append(f'{name}: IO publication depends on a weak view')
    document = mask_rust(sources['crates/zipzi-editor-core/src/document.rs'])
    for field, target in [('session', 'HostedSession'), ('edit', 'HostedEdit')]:
        if not re.search(rf'\b{field}\s*:\s*(?:crate::runtime_host::)?{target}\b', document):
            errors.append(f'Document must refer to {target}')
    native = mask_rust(sources['crates/zipzi-editor/src/lib.rs'])
    if 'on_window_closed' in native:
        errors.append('last-window lifecycle must not implicitly terminate the host')
    if '--no-window' not in sources['crates/zipzi-editor/src/lib.rs']:
        errors.append('missing no-window launch path')
    return errors


def main() -> int:
    errors = violations(collect())
    for error in errors:
        print(error)
    if not errors:
        print('R2 Editor ownership boundary: passed (source guard + separate behavioral gates)')
    return bool(errors)


if __name__ == '__main__':
    raise SystemExit(main())
