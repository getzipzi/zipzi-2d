"""One page/shard router shared by rendering, audits and the installed catalog."""

SPLITS = {
    'scene': {'edits': ('SceneOperation', 'SceneEdit', 'SceneContentEdit', 'OrganizationEdit', 'ReparentEdit', 'AttachmentEdit')},
    'bindings': {'edits': ('BindingAuthoringEdit', 'BindingGridEdit', 'BlendAuthoringEdit', 'MeshGridEdit')},
    'addressing': {'kinds': ('ObjectKind',), 'patches': ('AddressPatch',), 'selectors': ('AddressSelector',)},
    'deletion': {'decisions': ('DeletionChoice', 'DeletionDecision', 'DeletionEffect', 'DeletionReason')},
}


def page_group(path: str, domains: tuple[str, ...], modeling_domains: tuple[str, ...]) -> str:
    parts = path.split('.')
    segment = parts[1] if len(parts) > 1 else ''
    if segment == 'modeling' and len(parts) > 2 and parts[2] in modeling_domains:
        if len(parts) > 3:
            for suffix, prefixes in SPLITS.get(parts[2], {}).items():
                if parts[3].startswith(prefixes):
                    return 'modeling_' + parts[2] + '_' + suffix
        return 'modeling_' + parts[2]
    if segment in {'Session', 'Edit'}:
        return 'runtime_' + segment.lower()
    return segment if segment in domains else 'runtime'
