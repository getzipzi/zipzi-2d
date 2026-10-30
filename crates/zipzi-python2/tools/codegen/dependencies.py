"""Deterministic type and domain dependency analysis for renderer imports."""


def references(value):
    if isinstance(value, dict):
        if value.get('kind') == 'ref':
            yield value['name']
        else:
            for child in value.values():
                yield from references(child)
    elif isinstance(value, list):
        for child in value:
            yield from references(child)


def domain_dependencies(definitions, domains):
    result = {domain: set() for domain in domains.values()}
    for source, domain in domains.items():
        for target in references(definitions[source]):
            dependency = domains.get(target)
            if dependency is not None and dependency != domain:
                result[domain].add(dependency)
    return {domain: tuple(sorted(targets)) for domain, targets in sorted(result.items())}


def strong_components(graph):
    """Return sorted strongly connected components, including singleton nodes."""
    nodes = set(graph)
    nodes.update(target for targets in graph.values() for target in targets)
    indices = {}
    low = {}
    stack = []
    on_stack = set()
    components = []
    def visit(node):
        indices[node] = low[node] = len(indices)
        stack.append(node)
        on_stack.add(node)
        for target in sorted(graph.get(node, ())):
            if target not in indices:
                visit(target)
                low[node] = min(low[node], low[target])
            elif target in on_stack:
                low[node] = min(low[node], indices[target])
        if low[node] == indices[node]:
            component = []
            while True:
                member = stack.pop()
                on_stack.remove(member)
                component.append(member)
                if member == node:
                    break
            components.append(tuple(sorted(component)))
    for node in sorted(nodes):
        if node not in indices:
            visit(node)
    return tuple(sorted(components))
